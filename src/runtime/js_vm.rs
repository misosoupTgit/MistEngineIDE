//! QuickJS-based JavaScript runtime for MistEngine
//!
//! Mistral 自作言語の代替として JavaScript (QuickJS) を使用します。
//! SDL2 レンダラー・入力・FPS 管理等のネイティブ処理は変更せず、
//! 薄いグルー層として JS から Rust API を呼べるようにします。
//!
//! アーキテクチャ:
//!   [JS スクリプト] → [rquickjs] → [Arc<Mutex<>> 共有] → [SDL2/Rust ネイティブ処理]

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use rquickjs::{Context, Function, Runtime};

use crate::runtime::vm::{DrawCmd, GameState};

// ── JsInterpreter ────────────────────────────────────────────────

/// QuickJS ベースのゲームスクリプトインタープリター
///
/// このインタープリターはスクリプトスレッド上で生成・実行される。
/// Runtime/Context は !Send のため、スレッドをまたいで送ってはいけない。
/// sdl_window.rs の script_thread 内でのみ生成すること。
pub struct JsInterpreter {
    _runtime: Runtime,
    context:  Context,
    /// draw() 呼び出し中に蓄積される描画コマンドのバッファ
    /// JS 側の draw.* クロージャが Arc を通じてここへ push する
    pub frame_cmds: Arc<Mutex<Vec<DrawCmd>>>,
}

impl JsInterpreter {
    /// スクリプト文字列からインタープリターを生成する。
    ///
    /// - プリアンブル (draw/input/math/Color API ラッパー) を注入
    /// - ユーザースクリプトのトップレベルコードを実行（変数宣言等）
    pub fn new(script: &str, state: &GameState) -> Result<Self, String> {
        let runtime = Runtime::new()
            .map_err(|e| format!("[QuickJS] Runtime 生成失敗: {}", e))?;
        let context = Context::full(&runtime)
            .map_err(|e| format!("[QuickJS] Context 生成失敗: {}", e))?;

        let frame_cmds: Arc<Mutex<Vec<DrawCmd>>> =
            Arc::new(Mutex::new(Vec::with_capacity(256)));

        // ── JS グローバル登録 ──────────────────────────────────────
        {
            let fc       = Arc::clone(&frame_cmds);
            let bg       = Arc::clone(&state.bg_color);
            let held     = Arc::clone(&state.held_keys);
            let console  = Arc::clone(&state.console);
            let fps_arc  = Arc::clone(&state.fps);
            let sw_arc   = Arc::clone(&state.screen_w);
            let sh_arc   = Arc::clone(&state.screen_h);

            let init_result = context.with(|ctx| {
                let g = ctx.globals();

                // print / debug
                register_print(&ctx, &g, Arc::clone(&console))?;

                // draw.* ネイティブバインディング
                register_draw(&ctx, &g, Arc::clone(&fc), Arc::clone(&bg))?;

                // input.* ネイティブバインディング
                register_input(&ctx, &g, Arc::clone(&held))?;

                // engine.* / math.* ネイティブバインディング
                register_engine_math(&ctx, &g, fps_arc, sw_arc, sh_arc)?;

                // JS プリアンブル（draw / input / math / Color / engine の JS ラッパー）
                if let Err(e) = ctx.eval::<(), _>(JS_PREAMBLE) {
                    if let Some(exception) = ctx.catch().as_exception() {
                        eprintln!("[QuickJS Preamble Exception] {:?}", exception);
                    }
                    return Err(e);
                }

                // ユーザースクリプト（トップレベルコード実行）
                if let Err(e) = ctx.eval::<(), _>(script) {
                    if let Some(exception) = ctx.catch().as_exception() {
                        eprintln!("[QuickJS Script Exception] {:?}", exception);
                    }
                    return Err(e);
                }

                // 評価後に、もしグローバルの draw が関数であれば、そこに api をコピーする
                if let Err(e) = ctx.eval::<(), _>(r#"
                    if (typeof globalThis.draw === 'function') {
                        Object.assign(globalThis.draw, __draw_api_internal);
                    }
                "#) {
                    if let Some(exception) = ctx.catch().as_exception() {
                        eprintln!("[QuickJS Post-script Injection Exception] {:?}", exception);
                    }
                    return Err(e);
                }

                Ok::<_, rquickjs::Error>(())
            });

            init_result.map_err(|e| {
                format!("[QuickJS] スクリプト初期化失敗: {:?}", e)
            })?;
        }

        Ok(JsInterpreter {
            _runtime: runtime,
            context,
            frame_cmds,
        })
    }

    /// JS の `ready()` 関数を呼ぶ（未定義なら無視）
    pub fn call_ready(&self) -> Result<(), String> {
        self.context.with(|ctx| {
            let g = ctx.globals();
            if let Ok(f) = g.get::<_, rquickjs::Function>("ready") {
                f.call::<_, ()>(())
                    .map_err(|e| format!("[QuickJS] ready() エラー: {:?}", e))?;
            }
            Ok::<_, String>(())
        })
    }

    /// JS の `update(delta)` 関数を呼ぶ
    pub fn call_update(&self, delta: f64) -> Result<(), String> {
        self.context.with(|ctx| {
            let g = ctx.globals();
            if let Ok(f) = g.get::<_, rquickjs::Function>("update") {
                if let Err(e) = f.call::<_, ()>((delta,)) {
                    if let Some(exception) = ctx.catch().as_exception() {
                        eprintln!("[QuickJS update() Exception] {:?}", exception);
                    }
                    return Err(format!("[QuickJS] update() エラー: {:?}", e));
                }
            }
            Ok::<_, String>(())
        })
    }

    /// JS の `draw()` 関数を呼ぶ。
    /// 呼び出し前に frame_cmds をクリアし、実行後のコマンド列を返す。
    pub fn call_draw(&self) -> Result<Vec<DrawCmd>, String> {
        // 前フレームの描画コマンドをクリア
        self.frame_cmds.lock().unwrap().clear();

        self.context.with(|ctx| {
            let g = ctx.globals();
            if let Ok(f) = g.get::<_, rquickjs::Function>("draw") {
                if let Err(e) = f.call::<_, ()>(()) {
                    if let Some(exception) = ctx.catch().as_exception() {
                        eprintln!("[QuickJS draw() Exception] {:?}", exception);
                    }
                    return Err(format!("[QuickJS] draw() エラー: {:?}", e));
                }
            }
            Ok::<_, String>(())
        })?;

        Ok(self.frame_cmds.lock().unwrap().clone())
    }
}

// ── ネイティブバインディング登録 ─────────────────────────────────

fn register_print<'js>(
    ctx:     &rquickjs::Ctx<'js>,
    globals: &rquickjs::Object<'js>,
    console: Arc<Mutex<Vec<String>>>,
) -> rquickjs::Result<()> {
    let con = Arc::clone(&console);
    globals.set(
        "__print",
        Function::new(ctx.clone(), move |s: String| {
            println!("{}", s);
            if let Ok(mut q) = con.lock() {
                q.push(s);
            }
            Ok::<_, rquickjs::Error>(())
        })?,
    )?;

    globals.set(
        "__debug",
        Function::new(ctx.clone(), move |s: String| {
            eprintln!("[debug] {}", s);
            Ok::<_, rquickjs::Error>(())
        })?,
    )?;

    Ok(())
}

fn register_draw<'js>(
    ctx:       &rquickjs::Ctx<'js>,
    globals:   &rquickjs::Object<'js>,
    frame_cmds: Arc<Mutex<Vec<DrawCmd>>>,
    bg_color:  Arc<Mutex<[f32; 4]>>,
) -> rquickjs::Result<()> {
    // マクロで Arc を clone して各クロージャへキャプチャ
    macro_rules! draw_fn {
        ($name:expr, $fc:expr, $body:expr) => {{
            let fc = Arc::clone(&$fc);
            globals.set($name, Function::new(ctx.clone(), move |x:f64,y:f64,r:f64,color:rquickjs::Array<'js>| {
                let cr = color.get::<f32>(0).unwrap_or(1.0);
                let cg = color.get::<f32>(1).unwrap_or(1.0);
                let cb = color.get::<f32>(2).unwrap_or(1.0);
                let ca = color.get::<f32>(3).unwrap_or(1.0);
                fc.lock().unwrap().push($body(x as f32, y as f32, r as f32, [cr, cg, cb, ca]));
                Ok::<_, rquickjs::Error>(())
            })?)?;
        }};
    }

    draw_fn!("__draw_circle", frame_cmds, |x, y, r, color| DrawCmd::Circle { x, y, r, color });
    draw_fn!("__draw_triangle", frame_cmds, |x, y, s, color| DrawCmd::Triangle { x, y, s, color });
    draw_fn!("__draw_diamond", frame_cmds, |x, y, s, color| DrawCmd::Diamond { x, y, s, color });
    draw_fn!("__draw_square", frame_cmds, |x, y, s, color| DrawCmd::Square { x, y, s, color });

    // draw.rect: x, y, w, h, color
    {
        let fc = Arc::clone(&frame_cmds);
        globals.set(
            "__draw_rect",
            Function::new(ctx.clone(), move |x:f64, y:f64, w:f64, h:f64, color:rquickjs::Array<'js>| {
                let cr = color.get::<f32>(0).unwrap_or(1.0);
                let cg = color.get::<f32>(1).unwrap_or(1.0);
                let cb = color.get::<f32>(2).unwrap_or(1.0);
                let ca = color.get::<f32>(3).unwrap_or(1.0);
                fc.lock().unwrap().push(DrawCmd::Rect {
                    x: x as f32, y: y as f32, w: w as f32, h: h as f32,
                    color: [cr, cg, cb, ca],
                });
                Ok::<_, rquickjs::Error>(())
            })?,
        )?;
    }

    // draw.line: x1, y1, x2, y2, color
    {
        let fc = Arc::clone(&frame_cmds);
        globals.set(
            "__draw_line",
            Function::new(ctx.clone(), move |x1:f64, y1:f64, x2:f64, y2:f64, color:rquickjs::Array<'js>| {
                let cr = color.get::<f32>(0).unwrap_or(1.0);
                let cg = color.get::<f32>(1).unwrap_or(1.0);
                let cb = color.get::<f32>(2).unwrap_or(1.0);
                let ca = color.get::<f32>(3).unwrap_or(1.0);
                fc.lock().unwrap().push(DrawCmd::Line {
                    x1: x1 as f32, y1: y1 as f32, x2: x2 as f32, y2: y2 as f32,
                    color: [cr, cg, cb, ca],
                });
                Ok::<_, rquickjs::Error>(())
            })?,
        )?;
    }

    // draw.polygon: x, y, s, sides, color
    {
        let fc = Arc::clone(&frame_cmds);
        globals.set(
            "__draw_polygon",
            Function::new(ctx.clone(), move |x:f64, y:f64, s:f64, sides:i32, color:rquickjs::Array<'js>| {
                let cr = color.get::<f32>(0).unwrap_or(1.0);
                let cg = color.get::<f32>(1).unwrap_or(1.0);
                let cb = color.get::<f32>(2).unwrap_or(1.0);
                let ca = color.get::<f32>(3).unwrap_or(1.0);
                fc.lock().unwrap().push(DrawCmd::Polygon {
                    x: x as f32, y: y as f32, s: s as f32,
                    sides: sides.max(3) as u32,
                    color: [cr, cg, cb, ca],
                });
                Ok::<_, rquickjs::Error>(())
            })?,
        )?;
    }

    // draw.text: x, y, text, size, color
    {
        let fc = Arc::clone(&frame_cmds);
        globals.set(
            "__draw_text",
            Function::new(ctx.clone(), move |x:f64, y:f64, text:String, size:f64, color:rquickjs::Array<'js>| {
                let cr = color.get::<f32>(0).unwrap_or(1.0);
                let cg = color.get::<f32>(1).unwrap_or(1.0);
                let cb = color.get::<f32>(2).unwrap_or(1.0);
                let ca = color.get::<f32>(3).unwrap_or(1.0);
                fc.lock().unwrap().push(DrawCmd::Text {
                    x: x as f32, y: y as f32, text,
                    size: size as f32,
                    color: [cr, cg, cb, ca],
                });
                Ok::<_, rquickjs::Error>(())
            })?,
        )?;
    }

    // draw.image: x, y, path, w, h
    {
        let fc = Arc::clone(&frame_cmds);
        globals.set(
            "__draw_image",
            Function::new(ctx.clone(), move |x:f64, y:f64, path:String, w:f64, h:f64| {
                fc.lock().unwrap().push(DrawCmd::Image {
                    x: x as f32, y: y as f32, path,
                    w: w as f32, h: h as f32,
                });
                Ok::<_, rquickjs::Error>(())
            })?,
        )?;
    }

    // draw.background: color (bg_colorも更新)
    {
        let fc  = Arc::clone(&frame_cmds);
        let bg  = Arc::clone(&bg_color);
        globals.set(
            "__draw_background",
            Function::new(ctx.clone(), move |color:rquickjs::Array<'js>| {
                let cr = color.get::<f32>(0).unwrap_or(1.0);
                let cg = color.get::<f32>(1).unwrap_or(1.0);
                let cb = color.get::<f32>(2).unwrap_or(1.0);
                let ca = color.get::<f32>(3).unwrap_or(1.0);
                let color_arr = [cr, cg, cb, ca];
                *bg.lock().unwrap() = color_arr;
                fc.lock().unwrap().push(DrawCmd::Background(color_arr));
                Ok::<_, rquickjs::Error>(())
            })?,
        )?;
    }

    Ok(())
}

fn register_input<'js>(
    ctx:       &rquickjs::Ctx<'js>,
    globals:   &rquickjs::Object<'js>,
    held_keys: Arc<Mutex<std::collections::HashSet<String>>>,
) -> rquickjs::Result<()> {
    let hk = Arc::clone(&held_keys);
    globals.set(
        "__input_action_held",
        Function::new(ctx.clone(), move |action: String| {
            Ok::<bool, rquickjs::Error>(hk.lock().unwrap().contains(&action))
        })?,
    )?;

    let hk2 = Arc::clone(&held_keys);
    globals.set(
        "__input_action_pressed",
        Function::new(ctx.clone(), move |action: String| {
            // 簡略実装: held と同じ（pressed の正確な判定は SDL イベントで管理）
            Ok::<bool, rquickjs::Error>(hk2.lock().unwrap().contains(&action))
        })?,
    )?;

    globals.set(
        "__input_action_released",
        Function::new(ctx.clone(), move |_action: String| {
            // 簡略実装: held_keys に残らないため常に false
            Ok::<bool, rquickjs::Error>(false)
        })?,
    )?;

    Ok(())
}

fn register_engine_math<'js>(
    ctx:    &rquickjs::Ctx<'js>,
    globals: &rquickjs::Object<'js>,
    fps:    Arc<std::sync::atomic::AtomicU32>,
    sw:     Arc<std::sync::atomic::AtomicU32>,
    sh:     Arc<std::sync::atomic::AtomicU32>,
) -> rquickjs::Result<()> {
    // engine
    globals.set("__engine_fps",    Function::new(ctx.clone(), move || Ok::<f64, rquickjs::Error>(fps.load(Ordering::Relaxed) as f64))?)?;
    globals.set("__engine_width",  Function::new(ctx.clone(), move || Ok::<f64, rquickjs::Error>(sw.load(Ordering::Relaxed) as f64))?)?;
    globals.set("__engine_height", Function::new(ctx.clone(), move || Ok::<f64, rquickjs::Error>(sh.load(Ordering::Relaxed) as f64))?)?;

    // math — Rust 実装で精度保証
    globals.set("__math_sin",  Function::new(ctx.clone(), |x: f64| Ok::<f64, rquickjs::Error>(x.sin()))?)?;
    globals.set("__math_cos",  Function::new(ctx.clone(), |x: f64| Ok::<f64, rquickjs::Error>(x.cos()))?)?;
    globals.set("__math_tan",  Function::new(ctx.clone(), |x: f64| Ok::<f64, rquickjs::Error>(x.tan()))?)?;
    globals.set("__math_sqrt", Function::new(ctx.clone(), |x: f64| Ok::<f64, rquickjs::Error>(x.sqrt()))?)?;
    globals.set("__math_abs",  Function::new(ctx.clone(), |x: f64| Ok::<f64, rquickjs::Error>(x.abs()))?)?;
    globals.set("__math_rand", Function::new(ctx.clone(), || Ok::<f64, rquickjs::Error>(fast_rand()))?)?;
    globals.set(
        "__math_rand_int",
        Function::new(ctx.clone(), |lo: f64, hi: f64| {
            Ok::<f64, rquickjs::Error>(lo.floor() + (fast_rand() * (hi - lo).max(1.0)).floor())
        })?,
    )?;

    Ok(())
}

// ── 乱数生成器（外部依存なし・高速 xoshiro256） ─────────────────

fn fast_rand() -> f64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static S: AtomicU64 = AtomicU64::new(123_456_789_012_345_678);
    let s = S.fetch_add(0x9e37_79b9_7f4a_7c15, Ordering::Relaxed);
    let mut x = s ^ (s >> 30);
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 31;
    (x >> 11) as f64 / (1u64 << 53) as f64
}

// ── JS プリアンブル ───────────────────────────────────────────────
//
// ユーザースクリプトの前に自動注入される JS コード。
// draw / input / math / engine / Color の使いやすい API を定義する。
// ネイティブバインディング (__draw_circle 等) は Rust 側で登録済み。

const JS_PREAMBLE: &str = r##"
"use strict";

// ── カラー正規化ヘルパー ──────────────────────────────────────────
// 受け付けるフォーマット:
//   配列 [r, g, b] / [r, g, b, a]  (0-1 または 0-255 を自動判定)
//   文字列 "#RRGGBB" / "#RRGGBBAA" / "#RGB"
function __parse_color(c) {
    if (c == null || c === undefined) return [1, 1, 1, 1];
    if (Array.isArray(c)) {
        const scale = c.some(v => v > 1.0) ? 255.0 : 1.0;
        const a = c.length > 3 ? c[3] / scale : 1.0;
        return [c[0] / scale, (c[1] || 0) / scale, (c[2] || 0) / scale, a];
    }
    if (typeof c === 'string') {
        const h = c.replace('#', '');
        if (h.length === 3) {
            return [
                parseInt(h[0] + h[0], 16) / 255,
                parseInt(h[1] + h[1], 16) / 255,
                parseInt(h[2] + h[2], 16) / 255,
                1,
            ];
        }
        if (h.length === 6) {
            return [
                parseInt(h.slice(0, 2), 16) / 255,
                parseInt(h.slice(2, 4), 16) / 255,
                parseInt(h.slice(4, 6), 16) / 255,
                1,
            ];
        }
        if (h.length === 8) {
            return [
                parseInt(h.slice(0, 2), 16) / 255,
                parseInt(h.slice(2, 4), 16) / 255,
                parseInt(h.slice(4, 6), 16) / 255,
                parseInt(h.slice(6, 8), 16) / 255,
            ];
        }
    }
    return [1, 1, 1, 1];
}

// ── draw API ────────────────────────────────────────────────────
// ── draw API ────────────────────────────────────────────────────
const __draw_api_internal = {
    circle:     (x, y, r, color) => {
        const c = __parse_color(color);
        __draw_circle(+x, +y, +r, c);
    },
    rect:       (x, y, w, h, color) => {
        const c = __parse_color(color);
        __draw_rect(+x, +y, +w, +h, c);
    },
    square:     (x, y, s, color) => {
        const c = __parse_color(color);
        __draw_square(+x, +y, +s, c);
    },
    line:       (x1, y1, x2, y2, color) => {
        const c = __parse_color(color);
        __draw_line(+x1, +y1, +x2, +y2, c);
    },
    triangle:   (x, y, s, color) => {
        const c = __parse_color(color);
        __draw_triangle(+x, +y, +s, c);
    },
    polygon:    (x, y, s, sides, color) => {
        const c = __parse_color(color);
        __draw_polygon(+x, +y, +s, Math.floor(+sides) || 6, c);
    },
    diamond:    (x, y, s, color) => {
        const c = __parse_color(color);
        __draw_diamond(+x, +y, +s, c);
    },
    text:       (x, y, text, size, color) => {
        const c = __parse_color(color);
        __draw_text(+x, +y, String(text), +(size || 24), c);
    },
    image:      (x, y, path, w, h) => {
        __draw_image(+x, +y, String(path), +(w || 0), +(h || 0));
    },
    background: (color) => {
        const c = __parse_color(color);
        __draw_background(c);
    },
};
globalThis.draw = __draw_api_internal;

// ── Color 定数 ────────────────────────────────────────────────────
const Color = {
    RED:     [1, 0, 0, 1],
    GREEN:   [0, 1, 0, 1],
    BLUE:    [0, 0, 1, 1],
    WHITE:   [1, 1, 1, 1],
    BLACK:   [0, 0, 0, 1],
    YELLOW:  [1, 1, 0, 1],
    CYAN:    [0, 1, 1, 1],
    MAGENTA: [1, 0, 1, 1],
    from_hex: (hex) => __parse_color(String(hex)),
    rgba:     (r, g, b, a) => [+r / 255, +g / 255, +b / 255, a != null ? +a / 255 : 1],
};

// ── input API ─────────────────────────────────────────────────────
const input = {
    action_held:     (action) => __input_action_held(String(action)),
    action_pressed:  (action) => __input_action_pressed(String(action)),
    action_released: (action) => __input_action_released(String(action)),
    is_action_held:  (action) => __input_action_held(String(action)),
    held:            (action) => __input_action_held(String(action)),
    pressed:         (action) => __input_action_pressed(String(action)),
};

// ── math API ─────────────────────────────────────────────────────
const math = {
    PI:       3.141592653589793,
    TAU:      6.283185307179586,
    E:        2.718281828459045,
    INF:      Infinity,
    sin:      (x) => __math_sin(+x),
    cos:      (x) => __math_cos(+x),
    tan:      (x) => __math_tan(+x),
    sqrt:     (x) => __math_sqrt(+x),
    abs:      (x) => __math_abs(+x),
    floor:    (x) => Math.floor(x),
    ceil:     (x) => Math.ceil(x),
    round:    (x) => Math.round(x),
    log:      (x) => Math.log(x),
    sign:     (x) => Math.sign(x),
    pow:      (x, y) => Math.pow(x, y),
    max:      (x, y) => Math.max(x, y),
    min:      (x, y) => Math.min(x, y),
    clamp:    (x, lo, hi) => Math.min(Math.max(x, lo), hi),
    lerp:     (a, b, t) => a + (b - a) * t,
    rand:     () => __math_rand(),
    rand_int: (lo, hi) => Math.floor(__math_rand_int(+lo, +hi)),
    atan2:    (y, x) => Math.atan2(y, x),
    hypot:    (x, y) => Math.hypot(x, y),
};

// ── engine API ───────────────────────────────────────────────────
const engine = {
    fps:    () => __engine_fps(),
    width:  () => __engine_width(),
    height: () => __engine_height(),
};

// ── グローバルユーティリティ ──────────────────────────────────────

function print(...args) {
    __print(args.map(v => (v === null ? 'null' : String(v))).join(' '));
}
function debug(...args) {
    __debug(args.map(v => (v === null ? 'null' : String(v))).join(' '));
}

// Mistral 互換: rotate(current, delta) → 0-360 に正規化した角度
function rotate(current, delta) {
    return (((current + (delta || 0)) % 360) + 360) % 360;
}

// Mistral 互換: move_forward(x, y, steps, angle?) → [new_x, new_y]
// 角度定義: 0=上, 90=右, 180=下, 270=左 (Scratch 互換)
function move_forward(x, y, steps, angle) {
    if (angle == null) angle = 90;
    const rad = angle * Math.PI / 180;
    return [x + steps * Math.sin(rad), y + steps * (-Math.cos(rad))];
}

// wait() はゲームループが時間を管理するため no-op
function wait(secs) {}

// str / int / float 型変換ヘルパー
function str(v)   { return String(v); }
function int(v)   { return Math.trunc(+v) || 0; }
function float(v) { return +v || 0.0; }
function bool(v)  { return !!v; }

"##;
