use std::sync::atomic::Ordering;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Instant, Duration};

use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Keycode;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::{Point, Rect};
use sdl2::render::{BlendMode, Canvas, Texture, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::runtime::vm::{DrawCmd, GameState, Interpreter, Value};
use crate::runtime::input::InputConfig;

// テクスチャキャッシュ: パス → (w, h, テクスチャ)
type TexCache<'a> = HashMap<String, (u32, u32)>;

pub struct GameWindowConfig {
    pub title:      String,
    pub width:      u32,
    pub height:     u32,
    pub high_dpi:   bool,
    pub anti_alias: f32,
    pub resizable:  bool,
    pub vsync:      bool,
    pub proj_dir:   PathBuf,
}

fn to_sdl_color(c: &[f32; 4]) -> Color {
    Color::RGBA((c[0]*255.0) as u8,(c[1]*255.0) as u8,(c[2]*255.0) as u8,(c[3]*255.0) as u8)
}

fn fill_circle(canvas: &mut Canvas<Window>, cx: i32, cy: i32, r: i32) {
    if r <= 0 { return; }
    // draw_line N回 → fill_rects 1回にまとめて SDL2 内部の呼び出しオーバーヘッドを削減
    let cap = (r * 4 + 4) as usize;
    let mut rects: Vec<Rect> = Vec::with_capacity(cap);
    let (mut x, mut y, mut d) = (0i32, r, 3 - 2*r);
    while x <= y {
        rects.push(Rect::new(cx-y, cy+x, (2*y+1) as u32, 1));
        if x != 0 { rects.push(Rect::new(cx-y, cy-x, (2*y+1) as u32, 1)); }
        if x != y { rects.push(Rect::new(cx-x, cy+y, (2*x+1) as u32, 1)); }
        if x != 0 && x != y { rects.push(Rect::new(cx-x, cy-y, (2*x+1) as u32, 1)); }
        if d < 0 { d += 4*x+6; } else { y -= 1; d += 4*(x-y)+10; }
        x += 1;
    }
    let _ = canvas.fill_rects(&rects);
}

fn fill_polygon(canvas: &mut Canvas<Window>, pts: &[(f32,f32)]) {
    let n = pts.len(); if n < 3 { return; }
    let min_y = pts.iter().map(|p|p.1).fold(f32::INFINITY, f32::min) as i32;
    let max_y = pts.iter().map(|p|p.1).fold(f32::NEG_INFINITY, f32::max) as i32;
    let height = (max_y - min_y + 1).max(0) as usize;
    // 全スキャンラインの Rect をまとめて fill_rects へ（FFI 呼び出し1回）
    let mut rects: Vec<Rect> = Vec::with_capacity(height);
    let mut xs = [0i32; 64];
    for y in min_y..=max_y {
        let mut cnt = 0usize;
        let yf = y as f32;
        for i in 0..n {
            let (x1,y1)=pts[i]; let (x2,y2)=pts[(i+1)%n];
            if (y1<=yf&&y2>yf)||(y2<=yf&&y1>yf) {
                if cnt < xs.len() { xs[cnt]=(x1+(yf-y1)/(y2-y1)*(x2-x1)) as i32; cnt+=1; }
            }
        }
        xs[..cnt].sort_unstable();
        let mut i=0;
        while i+1<cnt {
            let lx=xs[i]; let rx=xs[i+1];
            if rx>=lx { rects.push(Rect::new(lx, y, (rx-lx+1) as u32, 1)); }
            i+=2;
        }
    }
    let _ = canvas.fill_rects(&rects);
}

/// 画面外カリング判定（AABB vs 画面矩形）
#[inline(always)]
fn is_visible(cmd: &DrawCmd, scale: f32, sw: f32, sh: f32) -> bool {
    match cmd {
        DrawCmd::Background(_) => true,
        DrawCmd::Circle{x,y,r,..} => {
            let (cx,cy,cr)=(x*scale,y*scale,r*scale);
            cx+cr>=0.0 && cx-cr<=sw && cy+cr>=0.0 && cy-cr<=sh
        }
        DrawCmd::Rect{x,y,w,h,..} => {
            let (rx,ry,rw,rh)=(x*scale,y*scale,w*scale,h*scale);
            rx+rw>=0.0 && rx<=sw && ry+rh>=0.0 && ry<=sh
        }
        DrawCmd::Square{x,y,s,..} => {
            let (sx2,sy2,ss)=(x*scale,y*scale,s*scale);
            sx2+ss>=0.0 && sx2<=sw && sy2+ss>=0.0 && sy2<=sh
        }
        DrawCmd::Line{x1,y1,x2,y2,..} => {
            let (ax,ay,bx,by)=(x1*scale,y1*scale,x2*scale,y2*scale);
            ax.max(bx)>=0.0 && ax.min(bx)<=sw && ay.max(by)>=0.0 && ay.min(by)<=sh
        }
        DrawCmd::Triangle{x,y,s,..} => {
            let (tx,ty,ts)=(x*scale,y*scale,s*scale);
            tx+ts>=0.0 && tx-ts<=sw && ty+ts>=0.0 && ty-ts<=sh
        }
        DrawCmd::Polygon{x,y,s,..} | DrawCmd::Diamond{x,y,s,..} => {
            let (px2,py2,ps)=(x*scale,y*scale,s*scale);
            px2+ps>=0.0 && px2-ps<=sw && py2+ps>=0.0 && py2-ps<=sh
        }
        DrawCmd::Text{x,y,size,..} => {
            let (tx,ty,ts)=(x*scale,y*scale,size*scale*20.0);
            tx+ts>=0.0 && tx<=sw+ts && ty+ts>=0.0 && ty-ts<=sh
        }
        DrawCmd::Image{x,y,w,h,..} => {
            let (ix,iy)=(x*scale,y*scale);
            let (iw,ih)=((*w).max(1.0)*scale,(*h).max(1.0)*scale);
            ix+iw>=0.0 && ix<=sw && iy+ih>=0.0 && iy<=sh
        }
    }
}

fn render_geom(canvas: &mut Canvas<Window>, cmd: &DrawCmd, scale: f32) {
    match cmd {
        DrawCmd::Background(_)|DrawCmd::Text{..}|DrawCmd::Image{..} => {}
        DrawCmd::Circle{x,y,r,color} => {
            canvas.set_draw_color(to_sdl_color(color));
            fill_circle(canvas,(x*scale) as i32,(y*scale) as i32,(r*scale) as i32);
        }
        DrawCmd::Rect{x,y,w,h,color} => {
            canvas.set_draw_color(to_sdl_color(color));
            let _=canvas.fill_rect(Rect::new((x*scale) as i32,(y*scale) as i32,
                ((*w*scale) as u32).max(1),((*h*scale) as u32).max(1)));
        }
        DrawCmd::Square{x,y,s,color} => {
            canvas.set_draw_color(to_sdl_color(color));
            let _=canvas.fill_rect(Rect::new((x*scale) as i32,(y*scale) as i32,
                ((*s*scale) as u32).max(1),((*s*scale) as u32).max(1)));
        }
        DrawCmd::Line{x1,y1,x2,y2,color} => {
            canvas.set_draw_color(to_sdl_color(color));
            let _=canvas.draw_line(
                Point::new((x1*scale) as i32,(y1*scale) as i32),
                Point::new((x2*scale) as i32,(y2*scale) as i32));
        }
        DrawCmd::Triangle{x,y,s,color} => {
            canvas.set_draw_color(to_sdl_color(color));
            let hs=s*0.866_f32;
            fill_polygon(canvas,&[(*x*scale,(y-hs*0.667)*scale),
                ((x+s*0.5)*scale,(y+hs*0.333)*scale),((x-s*0.5)*scale,(y+hs*0.333)*scale)]);
        }
        DrawCmd::Polygon{x,y,s,sides,color} => {
            canvas.set_draw_color(to_sdl_color(color));
            let n=(*sides).max(3) as u32;
            // スタック上に確保（最大64頂点）
            let mut pts = [(0.0f32,0.0f32);64];
            let n2 = n.min(64) as usize;
            for i in 0..n2 {
                let a=std::f32::consts::TAU*i as f32/n as f32-std::f32::consts::FRAC_PI_2;
                pts[i]=((x+s*a.cos())*scale,(y+s*a.sin())*scale);
            }
            fill_polygon(canvas,&pts[..n2]);
        }
        DrawCmd::Diamond{x,y,s,color} => {
            canvas.set_draw_color(to_sdl_color(color));
            fill_polygon(canvas,&[(*x*scale,(y-s)*scale),((x+s)*scale,*y*scale),
                (*x*scale,(y+s)*scale),((x-s)*scale,*y*scale)]);
        }
    }
}

fn load_system_font() -> Option<fontdue::Font> {
    for path in &["C:/Windows/Fonts/segoeui.ttf","C:/Windows/Fonts/arial.ttf","C:/Windows/Fonts/tahoma.ttf"] {
        if let Ok(data) = std::fs::read(path) {
            if let Ok(font) = fontdue::Font::from_bytes(data.as_slice(), fontdue::FontSettings::default()) {
                return Some(font);
            }
        }
    }
    None
}

fn render_text(canvas: &mut Canvas<Window>, tc: &TextureCreator<WindowContext>,
    font: &fontdue::Font, text: &str, x: f32, y: f32, size: f32, color: [f32; 4], scale: f32) {
    let px = size * scale;
    let [r,g,b,a] = color;
    let (ri,gi,bi,ai) = ((r*255.0) as u8,(g*255.0) as u8,(b*255.0) as u8,(a*255.0) as u8);
    let ascent = font.horizontal_line_metrics(px).map(|m| m.ascent as i32).unwrap_or((px*0.75) as i32);
    let base_y = (y*scale) as i32 + ascent;
    let mut cur_x = (x*scale) as i32;
    for ch in text.chars() {
        let (m, bm) = font.rasterize(ch, px);
        if m.width == 0 { cur_x += m.advance_width as i32; continue; }
        let mut rgba = vec![0u8; m.width * m.height * 4];
        for (i, &al) in bm.iter().enumerate() {
            rgba[i*4]=ri; rgba[i*4+1]=gi; rgba[i*4+2]=bi;
            rgba[i*4+3]=((ai as u32 * al as u32)/255) as u8;
        }
        if let Ok(mut tex) = tc.create_texture_streaming(PixelFormatEnum::ABGR8888, m.width as u32, m.height as u32) {
            tex.set_blend_mode(BlendMode::Blend);
            let _ = tex.update(None, &rgba, m.width*4);
            let _ = canvas.copy(&tex, None,
                Rect::new(cur_x+m.xmin as i32, base_y-m.height as i32-m.ymin as i32, m.width as u32, m.height as u32));
        }
        cur_x += m.advance_width as i32;
    }
}

// 画像ピクセルキャッシュ（ファイル読み込みを1回に）
type ImageCache = HashMap<String, (u32, u32, Vec<u8>)>;

fn render_image<'a>(
    canvas: &mut Canvas<Window>,
    tc: &'a TextureCreator<WindowContext>,
    cache: &mut ImageCache,
    tex_cache: &mut HashMap<String, sdl2::render::Texture<'a>>,
    path: &str, x: f32, y: f32, w: f32, h: f32, scale: f32,
) {
    // ピクセルデータをキャッシュ
    if !cache.contains_key(path) {
        let entry = if let Ok(img) = image::open(path) {
            let rgba = img.to_rgba8(); let (iw,ih) = rgba.dimensions();
            (iw, ih, rgba.into_raw())
        } else { (1,1,vec![255,0,255,255]) };
        cache.insert(path.to_string(), entry);
        // テクスチャも再生成
        tex_cache.remove(path);
    }
    // テクスチャをキャッシュ（毎フレーム生成を防止）
    if !tex_cache.contains_key(path) {
        if let Some((iw,ih,data)) = cache.get(path) {
            if let Ok(mut tex) = tc.create_texture_streaming(PixelFormatEnum::ABGR8888, *iw, *ih) {
                tex.set_blend_mode(BlendMode::Blend);
                let _ = tex.update(None, data, (*iw*4) as usize);
                tex_cache.insert(path.to_string(), tex);
            }
        }
    }
    if let (Some((iw,ih,_)), Some(tex)) = (cache.get(path), tex_cache.get(path)) {
        let dw = if w>0.0 {(w*scale) as u32} else {(*iw as f32*scale) as u32};
        let dh = if h>0.0 {(h*scale) as u32} else {(*ih as f32*scale) as u32};
        let _ = canvas.copy(tex, None, Rect::new((x*scale) as i32,(y*scale) as i32,dw.max(1),dh.max(1)));
    }
}

fn keycode_to_key_str(k: Keycode) -> Option<&'static str> {
    Some(match k {
        Keycode::A=>"Key.A", Keycode::B=>"Key.B", Keycode::C=>"Key.C", Keycode::D=>"Key.D",
        Keycode::E=>"Key.E", Keycode::F=>"Key.F", Keycode::G=>"Key.G", Keycode::H=>"Key.H",
        Keycode::I=>"Key.I", Keycode::J=>"Key.J", Keycode::K=>"Key.K", Keycode::L=>"Key.L",
        Keycode::M=>"Key.M", Keycode::N=>"Key.N", Keycode::O=>"Key.O", Keycode::P=>"Key.P",
        Keycode::Q=>"Key.Q", Keycode::R=>"Key.R", Keycode::S=>"Key.S", Keycode::T=>"Key.T",
        Keycode::U=>"Key.U", Keycode::V=>"Key.V", Keycode::W=>"Key.W", Keycode::X=>"Key.X",
        Keycode::Y=>"Key.Y", Keycode::Z=>"Key.Z",
        Keycode::Up=>"Key.Up", Keycode::Down=>"Key.Down",
        Keycode::Left=>"Key.Left", Keycode::Right=>"Key.Right",
        Keycode::Space=>"Key.Space", Keycode::Return=>"Key.Enter",
        Keycode::Escape=>"Key.Escape", Keycode::Tab=>"Key.Tab",
        Keycode::LShift|Keycode::RShift=>"Key.Shift",
        Keycode::LCtrl|Keycode::RCtrl=>"Key.Ctrl",
        Keycode::LAlt|Keycode::RAlt=>"Key.Alt",
        _=>return None,
    })
}

pub fn run_game_window(config: GameWindowConfig, mut interp: Interpreter, state: GameState) {
    // ── SDL2 セットアップ（メインスレッド必須） ──────────────────
    if config.high_dpi { sdl2::hint::set("SDL_WINDOWS_DPI_AWARENESS", "permonitorv2"); }

    let aa_scale: u32 = if config.anti_alias >= 2.0 {
        sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "best");
        config.anti_alias.ceil() as u32
    } else if config.anti_alias > 0.0 {
        sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "linear"); 1
    } else {
        sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "nearest"); 1
    };

    let sdl   = sdl2::init().expect("SDL2 init failed");
    let video = sdl.video().expect("SDL2 video failed");
    let mut wb = video.window(&config.title, config.width, config.height);
    wb.position_centered();
    if config.resizable { wb.resizable(); }
    if config.high_dpi  { wb.allow_highdpi(); }
    let window = wb.build().expect("window failed");
    let mut cb = window.into_canvas().accelerated().target_texture();
    if config.vsync { cb = cb.present_vsync(); }
    let mut canvas = cb.build().expect("canvas failed");
    canvas.set_blend_mode(BlendMode::Blend);

    let (phys_w, phys_h) = canvas.output_size().unwrap_or((config.width, config.height));
    let dpi_scale = phys_w as f32 / config.width as f32;
    state.screen_w.store(phys_w, Ordering::Relaxed);
    state.screen_h.store(phys_h, Ordering::Relaxed);

    let geom_scale = dpi_scale * aa_scale as f32;
    let tex_creator = canvas.texture_creator();
    let mut aa_tex = if aa_scale > 1 {
        Some(tex_creator.create_texture_target(PixelFormatEnum::RGBA8888, phys_w*aa_scale, phys_h*aa_scale)
            .expect("AA texture failed"))
    } else { None };

    // input.json → キー逆引きマップ
    let input_cfg = InputConfig::load(&config.proj_dir.join("input.json"))
        .unwrap_or_else(|_| InputConfig::default_config());
    let mut key_to_actions: HashMap<String, Vec<String>> = HashMap::new();
    for (action, keys) in &input_cfg.keys {
        for k in keys { key_to_actions.entry(k.clone()).or_default().push(action.clone()); }
    }

    let font      = load_system_font();
    let mut img_cache: ImageCache = HashMap::new();
    // unsafe: テクスチャのライフタイムを canvas と同じにするための回避策
    // sdl2 のテクスチャは TextureCreator に紐づくため unsafe で延長する
    let mut tex_cache: HashMap<String, sdl2::render::Texture> = HashMap::new();
    // draw_cmds の swap 用ローカルバッファ（毎フレームのヒープ確保を排除）
    let mut local_cmds: Vec<DrawCmd> = Vec::with_capacity(128);
    let mut key_changed = false;
    // 最後にレンダリングした frame_id（0 = まだ描画していない）
    let mut last_rendered_id: u64 = u64::MAX;

    // ── スクリプトスレッド起動 ───────────────────────────────────
    // wait() はスクリプトスレッドだけブロック → 描画スレッドは継続
    let state2 = state.clone_arcs();
    let script_thread = std::thread::spawn(move || {
        if let Some(func) = interp.get_var("ready") {
            if let Err(e) = interp.call_value(func, vec![]) {
                eprintln!("[Mistral] ready() error: {}", e);
            }
        }
        let mut last = Instant::now();
        while state2.running.load(Ordering::Relaxed) {
            let now = Instant::now();
            let dt  = now.duration_since(last).as_secs_f64();
            last = now;

            if let Some(func) = interp.get_var("update") {
                if let Err(e) = interp.call_value(func, vec![Value::Float(dt)]) {
                    eprintln!("[Mistral] update() error: {}", e); break;
                }
            }
            interp.frame_cmds.clear();
            if let Some(func) = interp.get_var("draw") {
                if let Err(e) = interp.call_value(func, vec![]) {
                    eprintln!("[Mistral] draw() error: {}", e); break;
                }
            }
            // draw_cmds を共有ステートへ公開 + frame_id をインクリメント
            *state2.draw_cmds.lock().unwrap() = std::mem::take(&mut interp.frame_cmds);
            state2.frame_id.fetch_add(1, Ordering::Release);

            if let Ok(mut q) = state2.console.try_lock() {
                for l in q.drain(..) { println!("{}", l); }
            }
        }
        state2.running.store(false, Ordering::Relaxed);
    });

    // ── 描画ループ（メインスレッド） ─────────────────────────────
    let mut events  = sdl.event_pump().expect("event pump failed");
    let mut held    : HashSet<String> = HashSet::new();
    let mut fps_acc = 0.0f64;
    let mut fps_cnt = 0u32;
    let mut last_fps = Instant::now();

    'main: loop {
        if !state.running.load(Ordering::Relaxed) { break; }

        key_changed = false;
        for ev in events.poll_iter() {
            match ev {
                Event::Quit{..} => { state.running.store(false,Ordering::Relaxed); break 'main; }
                Event::KeyDown{keycode:Some(k),repeat:false,..} => {
                    if let Some(ks) = keycode_to_key_str(k) {
                        if let Some(acts) = key_to_actions.get(ks) {
                            for a in acts { held.insert(a.clone()); }
                            key_changed = true;
                        }
                    }
                }
                Event::KeyUp{keycode:Some(k),..} => {
                    if let Some(ks) = keycode_to_key_str(k) {
                        if let Some(acts) = key_to_actions.get(ks) {
                            for a in acts { held.remove(a); }
                            key_changed = true;
                        }
                    }
                }
                Event::Window{win_event:WindowEvent::Resized(w,h),..} => {
                    let (pw,ph) = canvas.output_size().unwrap_or((w as u32, h as u32));
                    state.screen_w.store(pw,Ordering::Relaxed);
                    state.screen_h.store(ph,Ordering::Relaxed);
                }
                _ => {}
            }
        }
        // キー変化時のみロックして書き込み（毎フレームcloneを排除）
        if key_changed {
            if let Ok(mut hk) = state.held_keys.lock() { hk.clone_from(&held); }
        }

        // FPS カウント
        let now = Instant::now();
        let dt  = now.duration_since(last_fps).as_secs_f64();
        last_fps = now;
        fps_acc += dt; fps_cnt += 1;
        if fps_acc >= 0.5 {
            state.fps.store((fps_cnt as f64/fps_acc).round() as u32, Ordering::Relaxed);
            fps_acc = 0.0; fps_cnt = 0;
        }

        // 新フレームがあれば draw_cmds を swap
        let current_id = state.frame_id.load(Ordering::Acquire);
        let new_frame  = current_id != last_rendered_id;
        if new_frame {
            if let Ok(mut q) = state.draw_cmds.try_lock() {
                std::mem::swap(&mut *q, &mut local_cmds);
            }
            last_rendered_id = current_id;
        }

        let (sw, sh) = (
            state.screen_w.load(Ordering::Relaxed) as f32,
            state.screen_h.load(Ordering::Relaxed) as f32,
        );

        // 同一フレームなら描画パイプラインを全スキップ——present()のみ呼ぶ
        if !new_frame {
            canvas.present();
            continue;
        }

        let bg = local_cmds.iter().find_map(|c| if let DrawCmd::Background(col)=c { Some(to_sdl_color(col)) } else { None })
            .unwrap_or(Color::RGB(13,13,26));

        if let Some(ref mut tex) = aa_tex {
            canvas.with_texture_canvas(tex, |tc| {
                tc.set_blend_mode(BlendMode::Blend);
                tc.set_draw_color(bg); tc.clear();
                for cmd in &local_cmds {
                    if is_visible(cmd, geom_scale, sw*geom_scale/dpi_scale, sh*geom_scale/dpi_scale) {
                        render_geom(tc, cmd, geom_scale);
                    }
                }
            }).ok();
            canvas.set_draw_color(bg); canvas.clear();
            canvas.copy(tex, None, None).ok();
        } else {
            canvas.set_draw_color(bg); canvas.clear();
            for cmd in &local_cmds {
                if is_visible(cmd, dpi_scale, sw, sh) {
                    render_geom(&mut canvas, cmd, dpi_scale);
                }
            }
        }

        for cmd in &local_cmds {
            match cmd {
                DrawCmd::Text{x,y,text,size,color} => {
                    if is_visible(cmd, dpi_scale, sw, sh) {
                        if let Some(ref f) = font {
                            render_text(&mut canvas, &tex_creator, f, text, *x, *y, *size, *color, dpi_scale);
                        }
                    }
                }
                DrawCmd::Image{x,y,path,w,h} => {
                    if is_visible(cmd, dpi_scale, sw, sh) {
                        render_image(&mut canvas, &tex_creator, &mut img_cache, &mut tex_cache, path, *x, *y, *w, *h, dpi_scale);
                    }
                }
                _ => {}
            }
        }

        canvas.present();
    }

    state.running.store(false, Ordering::Relaxed);
    // スクリプトスレッドが wait() で寝ていても最大500ms待つ
    let _ = std::thread::Builder::new().spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        drop(script_thread);
    });
}