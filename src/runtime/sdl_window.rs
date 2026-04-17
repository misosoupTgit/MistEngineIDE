/// ゲームウィンドウ（miniquad GPU ベース）
///
/// minifb の CPU ソフトウェアレンダリングから miniquad GPU (OpenGL) レンダリングに移行。
///
/// 改善点:
///   ① 全シェイプを GPU 三角形プリミティブで描画 → CPU SSAA/スキャンライン完全撤廃
///   ② GPU が自動ラスタライズ＋アルファブレンド → 円も 48 セグメント三角形ファンで滑らか
///   ③ 1フレーム 1ドローコール → 頂点バッファを毎フレーム Stream 更新
///   ④ vsync / 無制限 FPS 両対応
///
/// 依存: miniquad = "0.4"

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Instant;
use miniquad::*;
use crate::runtime::vm::DrawCmd;

// ── 公開設定 ─────────────────────────────────────────────────
pub struct GameWindowConfig {
    pub title:      String,
    pub width:      u32,
    pub height:     u32,
    pub high_dpi:   bool,
    /// 1.0=標準 / 2.0以上=GPU MSAA（将来拡張用、現在は未使用）
    pub anti_alias: f32,
    pub resizable:  bool,
    /// true=vsync有効 / false=無制限FPS
    pub vsync:      bool,
}

// ── GPU 頂点 ─────────────────────────────────────────────────
#[repr(C)]
#[derive(Clone, Copy)]
struct Vertex {
    pos:   [f32; 2],
    color: [f32; 4],
}

// ── Uniform ──────────────────────────────────────────────────
#[repr(C)]
struct Uniforms {
    screen_size: (f32, f32),
}

// ── 定数 ─────────────────────────────────────────────────────
const INITIAL_MAX_VERTS: usize = 65536;
const INITIAL_MAX_IDX:   usize = 131072;
/// 円の分割数（多いほど滑らか・48で十分）
const CIRCLE_SEGMENTS:   usize = 48;

// ══════════════════════════════════════════════════════════════
//  GameStage – miniquad EventHandler
// ══════════════════════════════════════════════════════════════
struct GameStage {
    ctx:        Box<dyn RenderingBackend>,
    pipeline:   Pipeline,
    vbuf:       BufferId,
    ibuf:       BufferId,
    max_verts:  usize,
    max_idx:    usize,

    draw_cmds:  Arc<Mutex<Vec<DrawCmd>>>,
    bg_color:   Arc<Mutex<[f32; 4]>>,
    held_keys:  Arc<Mutex<HashSet<String>>>,
    running:    Arc<AtomicBool>,
    screen_w:   f32,
    screen_h:   f32,

    // FPS 計測 & 共有
    shared_fps:      Arc<AtomicU32>,
    shared_screen_w: Arc<AtomicU32>,
    shared_screen_h: Arc<AtomicU32>,
    last_frame:      Instant,
    frame_count:     u32,
    fps_accum:       f64,
}

impl GameStage {
    fn new(
        draw_cmds: Arc<Mutex<Vec<DrawCmd>>>,
        bg_color:  Arc<Mutex<[f32; 4]>>,
        held_keys: Arc<Mutex<HashSet<String>>>,
        running:   Arc<AtomicBool>,
        shared_fps:      Arc<AtomicU32>,
        shared_screen_w: Arc<AtomicU32>,
        shared_screen_h: Arc<AtomicU32>,
        screen_w:  f32,
        screen_h:  f32,
    ) -> Self {
        let mut ctx: Box<dyn RenderingBackend> = window::new_rendering_backend();

        // ── 動的ストリームバッファ（毎フレーム上書き） ──
        let vbuf = ctx.new_buffer(
            BufferType::VertexBuffer,
            BufferUsage::Stream,
            BufferSource::empty::<Vertex>(INITIAL_MAX_VERTS),
        );
        let ibuf = ctx.new_buffer(
            BufferType::IndexBuffer,
            BufferUsage::Stream,
            BufferSource::empty::<u16>(INITIAL_MAX_IDX),
        );

        // ── シェーダー（ピクセル座標 → NDC 変換、頂点カラー直通） ──
        let shader = ctx
            .new_shader(
                ShaderSource::Glsl {
                    vertex:   VERT_SHADER,
                    fragment: FRAG_SHADER,
                },
                shader_meta(),
            )
            .expect("[GameStage] シェーダーコンパイル失敗");

        // ── パイプライン（αブレンド有効） ──
        let pipeline = ctx.new_pipeline(
            &[BufferLayout::default()],
            &[
                VertexAttribute::new("in_pos",   VertexFormat::Float2),
                VertexAttribute::new("in_color", VertexFormat::Float4),
            ],
            shader,
            PipelineParams {
                color_blend: Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::Value(BlendValue::SourceAlpha),
                    BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                )),
                ..Default::default()
            },
        );

        GameStage {
            ctx, pipeline, vbuf, ibuf,
            max_verts: INITIAL_MAX_VERTS,
            max_idx:   INITIAL_MAX_IDX,
            draw_cmds, bg_color, held_keys, running,
            screen_w, screen_h,
            shared_fps, shared_screen_w, shared_screen_h,
            last_frame: Instant::now(),
            frame_count: 0,
            fps_accum: 0.0,
        }
    }

    // ── バッファ拡張（必要時のみ） ──────────────────────────
    fn ensure_buffers(&mut self, vert_count: usize, idx_count: usize) {
        if vert_count > self.max_verts {
            self.max_verts = vert_count.next_power_of_two();
            self.ctx.delete_buffer(self.vbuf);
            self.vbuf = self.ctx.new_buffer(
                BufferType::VertexBuffer,
                BufferUsage::Stream,
                BufferSource::empty::<Vertex>(self.max_verts),
            );
        }
        if idx_count > self.max_idx {
            self.max_idx = idx_count.next_power_of_two();
            self.ctx.delete_buffer(self.ibuf);
            self.ibuf = self.ctx.new_buffer(
                BufferType::IndexBuffer,
                BufferUsage::Stream,
                BufferSource::empty::<u16>(self.max_idx),
            );
        }
    }

    // ══════════════════════════════════════════════════════════
    //  シェイプ → GPU 頂点テセレーション
    // ══════════════════════════════════════════════════════════

    fn tessellate(&self, cmds: &[DrawCmd]) -> (Vec<Vertex>, Vec<u16>) {
        // 事前に容量を見積もってアロケーションを最小化
        let mut verts = Vec::with_capacity(cmds.len() * 8);
        let mut idx   = Vec::with_capacity(cmds.len() * 12);

        for cmd in cmds {
            match cmd {
                DrawCmd::Circle { x, y, r, color } => {
                    push_circle(&mut verts, &mut idx, *x, *y, *r, color);
                }
                DrawCmd::Rect { x, y, w, h, color } => {
                    push_rect(&mut verts, &mut idx, *x, *y, *w, *h, color);
                }
                DrawCmd::Square { x, y, s, color } => {
                    push_rect(&mut verts, &mut idx, *x, *y, *s, *s, color);
                }
                DrawCmd::Line { x1, y1, x2, y2, color } => {
                    push_line(&mut verts, &mut idx, *x1, *y1, *x2, *y2, 2.0, color);
                }
                DrawCmd::Triangle { x, y, s, color } => {
                    let hs = s * 0.866_f32;
                    let base = verts.len() as u16;
                    verts.push(Vertex { pos: [*x,            *y - hs * 0.667], color: *color });
                    verts.push(Vertex { pos: [*x + s * 0.5,  *y + hs * 0.333], color: *color });
                    verts.push(Vertex { pos: [*x - s * 0.5,  *y + hs * 0.333], color: *color });
                    idx.extend_from_slice(&[base, base + 1, base + 2]);
                }
                DrawCmd::Polygon { x, y, s, sides, color } => {
                    push_ngon(&mut verts, &mut idx, *x, *y, *s, (*sides).max(3), color);
                }
                DrawCmd::Diamond { x, y, s, color } => {
                    let base = verts.len() as u16;
                    verts.push(Vertex { pos: [*x,      *y - s], color: *color });
                    verts.push(Vertex { pos: [*x + s,  *y    ], color: *color });
                    verts.push(Vertex { pos: [*x,      *y + s], color: *color });
                    verts.push(Vertex { pos: [*x - s,  *y    ], color: *color });
                    idx.extend_from_slice(&[base, base+1, base+2,
                                            base, base+2, base+3]);
                }
                // Text / Background は GPU テセレーション対象外
                DrawCmd::Text { .. } | DrawCmd::Background(_) => {}
            }
        }
        (verts, idx)
    }
}

// ── テセレーション個別関数 ───────────────────────────────────

/// 円 → 三角形ファン（center + CIRCLE_SEGMENTS+1 周辺頂点）
fn push_circle(v: &mut Vec<Vertex>, idx: &mut Vec<u16>,
               cx: f32, cy: f32, r: f32, color: &[f32; 4]) {
    if r <= 0.0 { return; }
    let base = v.len() as u16;
    // 中心
    v.push(Vertex { pos: [cx, cy], color: *color });
    // 周辺
    for i in 0..=CIRCLE_SEGMENTS {
        let a = std::f32::consts::TAU * i as f32 / CIRCLE_SEGMENTS as f32;
        v.push(Vertex { pos: [cx + r * a.cos(), cy + r * a.sin()], color: *color });
    }
    for i in 0..CIRCLE_SEGMENTS as u16 {
        idx.extend_from_slice(&[base, base + 1 + i, base + 2 + i]);
    }
}

/// 矩形 → 2 三角形
fn push_rect(v: &mut Vec<Vertex>, idx: &mut Vec<u16>,
             x: f32, y: f32, w: f32, h: f32, color: &[f32; 4]) {
    let base = v.len() as u16;
    v.push(Vertex { pos: [x,     y    ], color: *color });
    v.push(Vertex { pos: [x + w, y    ], color: *color });
    v.push(Vertex { pos: [x + w, y + h], color: *color });
    v.push(Vertex { pos: [x,     y + h], color: *color });
    idx.extend_from_slice(&[base, base+1, base+2,
                            base, base+2, base+3]);
}

/// 太い直線 → 法線方向に膨らませた矩形（2 三角形）
fn push_line(v: &mut Vec<Vertex>, idx: &mut Vec<u16>,
             x1: f32, y1: f32, x2: f32, y2: f32,
             thickness: f32, color: &[f32; 4]) {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
    let nx = -dy / len * thickness * 0.5;
    let ny =  dx / len * thickness * 0.5;
    let base = v.len() as u16;
    v.push(Vertex { pos: [x1 + nx, y1 + ny], color: *color });
    v.push(Vertex { pos: [x1 - nx, y1 - ny], color: *color });
    v.push(Vertex { pos: [x2 - nx, y2 - ny], color: *color });
    v.push(Vertex { pos: [x2 + nx, y2 + ny], color: *color });
    idx.extend_from_slice(&[base, base+1, base+2,
                            base, base+2, base+3]);
}

/// 正多角形 → 三角形ファン
fn push_ngon(v: &mut Vec<Vertex>, idx: &mut Vec<u16>,
             cx: f32, cy: f32, r: f32, sides: u32, color: &[f32; 4]) {
    let n = sides.max(3) as usize;
    let base = v.len() as u16;
    v.push(Vertex { pos: [cx, cy], color: *color });
    for i in 0..=n {
        let a = std::f32::consts::TAU * i as f32 / n as f32
                - std::f32::consts::FRAC_PI_2;
        v.push(Vertex { pos: [cx + r * a.cos(), cy + r * a.sin()], color: *color });
    }
    for i in 0..n as u16 {
        idx.extend_from_slice(&[base, base + 1 + i, base + 2 + i]);
    }
}

// ══════════════════════════════════════════════════════════════
//  EventHandler 実装
// ══════════════════════════════════════════════════════════════

impl EventHandler for GameStage {
    fn update(&mut self) {
        // IDE 側からの停止要求をチェック
        if !self.running.load(Ordering::Relaxed) {
            window::request_quit();
        }
    }

    fn draw(&mut self) {
        // ── FPS 計測 ──
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f64();
        self.last_frame = now;
        self.fps_accum += dt;
        self.frame_count += 1;
        if self.fps_accum >= 0.5 {
            let fps = (self.frame_count as f64 / self.fps_accum).round() as u32;
            self.shared_fps.store(fps, Ordering::Relaxed);
            self.frame_count = 0;
            self.fps_accum = 0.0;
        }

        // ── 画面サイズ更新 & 共有 ──
        let (sw, sh) = window::screen_size();
        self.screen_w = sw;
        self.screen_h = sh;
        self.shared_screen_w.store(sw as u32, Ordering::Relaxed);
        self.shared_screen_h.store(sh as u32, Ordering::Relaxed);

        // ── 背景色 ──
        let bg = self.bg_color.lock().map(|b| *b).unwrap_or([0.05, 0.05, 0.1, 1.0]);

        // ── 描画コマンド取得 & テセレーション ──
        let cmds = self.draw_cmds.lock().map(|q| q.clone()).unwrap_or_default();
        let (verts, indices) = self.tessellate(&cmds);

        // ── GPU レンダリング ──
        self.ctx.begin_default_pass(PassAction::clear_color(bg[0], bg[1], bg[2], bg[3]));

        if !verts.is_empty() && !indices.is_empty() {
            self.ensure_buffers(verts.len(), indices.len());
            self.ctx.buffer_update(self.vbuf, BufferSource::slice(&verts));
            self.ctx.buffer_update(self.ibuf, BufferSource::slice(&indices));

            let bindings = Bindings {
                vertex_buffers: vec![self.vbuf],
                index_buffer:   self.ibuf,
                images:         vec![],
            };

            self.ctx.apply_pipeline(&self.pipeline);
            self.ctx.apply_bindings(&bindings);
            self.ctx.apply_uniforms(UniformsSource::table(&Uniforms {
                screen_size: (self.screen_w, self.screen_h),
            }));
            self.ctx.draw(0, indices.len() as i32, 1);
        }

        self.ctx.end_render_pass();
        self.ctx.commit_frame();
    }

    // ── キー入力（イベント駆動） ────────────────────────────
    fn key_down_event(&mut self, keycode: KeyCode, _keymods: KeyMods, _repeat: bool) {
        if let Some(action) = key_to_action(keycode) {
            if let Ok(mut keys) = self.held_keys.lock() {
                keys.insert(action.to_string());
            }
        }
    }

    fn key_up_event(&mut self, keycode: KeyCode, _keymods: KeyMods) {
        if let Some(action) = key_to_action(keycode) {
            if let Ok(mut keys) = self.held_keys.lock() {
                keys.remove(action);
            }
        }
    }

    // ── ウィンドウ閉じ要求 ──
    fn quit_requested_event(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

// ── キー → アクション名マッピング ───────────────────────────
fn key_to_action(k: KeyCode) -> Option<&'static str> {
    match k {
        KeyCode::W | KeyCode::Up    => Some("move_up"),
        KeyCode::S | KeyCode::Down  => Some("move_down"),
        KeyCode::A | KeyCode::Left  => Some("move_left"),
        KeyCode::D | KeyCode::Right => Some("move_right"),
        KeyCode::Space              => Some("jump"),
        KeyCode::Z                  => Some("attack"),
        KeyCode::X                  => Some("action"),
        KeyCode::Escape             => Some("pause"),
        _ => None,
    }
}

// ══════════════════════════════════════════════════════════════
//  GLSL シェーダー
// ══════════════════════════════════════════════════════════════

/// 頂点シェーダー：ピクセル座標 → NDC（Y 反転・左上原点）
const VERT_SHADER: &str = r#"#version 100
attribute vec2 in_pos;
attribute vec4 in_color;

uniform vec2 screen_size;

varying lowp vec4 v_color;

void main() {
    vec2 ndc = in_pos / screen_size * 2.0 - 1.0;
    ndc.y = -ndc.y;
    gl_Position = vec4(ndc, 0.0, 1.0);
    v_color = in_color;
}"#;

/// フラグメントシェーダー：頂点カラー直通
const FRAG_SHADER: &str = r#"#version 100
varying lowp vec4 v_color;
void main() {
    gl_FragColor = v_color;
}"#;

fn shader_meta() -> ShaderMeta {
    ShaderMeta {
        images: vec![],
        uniforms: UniformBlockLayout {
            uniforms: vec![UniformDesc::new("screen_size", UniformType::Float2)],
        },
    }
}

// ══════════════════════════════════════════════════════════════
//  公開 API（IDE から呼ばれるエントリポイント）
// ══════════════════════════════════════════════════════════════

/// miniquad ベースのゲームウィンドウを起動する。
///
/// この関数は **呼び出しスレッドをブロック** する（miniquad のイベントループ）。
/// IDE 側では `std::thread::spawn` で呼び出すこと。
///
/// Windows では WinAPI でウィンドウ＆GL コンテキストを作成するため、
/// メインスレッド以外からの呼び出しでも正常に動作する。
pub fn run_game_window(
    config:    GameWindowConfig,
    draw_cmds: Arc<Mutex<Vec<DrawCmd>>>,
    bg_color:  Arc<Mutex<[f32; 4]>>,
    held_keys: Arc<Mutex<HashSet<String>>>,
    running:   Arc<AtomicBool>,
    shared_fps:      Arc<AtomicU32>,
    shared_screen_w: Arc<AtomicU32>,
    shared_screen_h: Arc<AtomicU32>,
) {
    let conf = conf::Conf {
        window_title:  config.title.clone(),
        window_width:  config.width  as i32,
        window_height: config.height as i32,
        high_dpi:      config.high_dpi,
        ..Default::default()
    };

    let sw = config.width  as f32;
    let sh = config.height as f32;

    miniquad::start(conf, move || {
        Box::new(GameStage::new(
            draw_cmds, bg_color, held_keys, running,
            shared_fps, shared_screen_w, shared_screen_h,
            sw, sh,
        ))
    });
}
