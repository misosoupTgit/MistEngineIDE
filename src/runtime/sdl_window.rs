/// ゲームウィンドウ（minifb ベース）
///
/// AA設計（大きな中間バッファは使わない）:
///   ① 円  → エッジゾーン限定 SSAA（内側は直接fill、外側はskip）
///            ssaa=4でも ~3,200 ops/円  ← 旧方式2.76M ops/frame比860×高速
///   ② 直線 → Xiaolin Wu サブピクセルAA（SSAA不要）
///   ③ ポリゴン → 解析的スキャンラインAA（エッジのみfloatブレンド）
///   ④ 矩形 → 高速ソリッドfill（AA不要）
///
/// anti_alias: 1.0=解析的AAのみ / 2.0=2×SSAA / 4.0=4×SSAA（エッジのみ）
/// vsync:      true=60fps上限 / false=無制限

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use minifb::{Key, Window, WindowOptions};
use crate::runtime::vm::DrawCmd;

pub struct GameWindowConfig {
    pub title:      String,
    pub width:      u32,
    pub height:     u32,
    pub high_dpi:   bool,
    /// 1.0=解析的AAのみ / 2.0=2×SSAA / 4.0=4×SSAA
    pub anti_alias: f32,
    pub resizable:  bool,
    /// true=60fps上限 / false=無制限
    pub vsync:      bool,
}

const KEY_MAP: &[(Key, &str)] = &[
    (Key::W,"move_up"),(Key::Up,"move_up"),
    (Key::S,"move_down"),(Key::Down,"move_down"),
    (Key::A,"move_left"),(Key::Left,"move_left"),
    (Key::D,"move_right"),(Key::Right,"move_right"),
    (Key::Space,"jump"),(Key::Z,"attack"),
    (Key::X,"action"),(Key::Escape,"pause"),
];

pub fn run_sdl2_window(
    config:    GameWindowConfig,
    draw_cmds: Arc<Mutex<Vec<DrawCmd>>>,
    bg_color:  Arc<Mutex<[f32; 4]>>,
    held_keys: Arc<Mutex<HashSet<String>>>,
    running:   Arc<AtomicBool>,
) {
    let dpi_s  = if config.high_dpi { system_dpi_scale() } else { 1.0_f32 };
    let aa_s   = config.anti_alias.max(1.0);
    // SSAA サンプリング数（エッジのみに適用）
    let ssaa   = dpi_s.max(aa_s).round() as usize;

    let win_w = config.width  as usize;
    let win_h = config.height as usize;

    let opts = WindowOptions { resize: config.resizable, ..WindowOptions::default() };
    let mut window = match Window::new(&config.title, win_w, win_h, opts) {
        Ok(w) => w, Err(e) => { eprintln!("[GameWindow] {e}"); return; }
    };

    // VSync 制御
    if config.vsync {
        window.limit_update_rate(Some(std::time::Duration::from_micros(16_600)));
    } else {
        window.limit_update_rate(None);  // 無制限
    }

    // バッファは常に論理サイズ（大きな中間バッファは使わない）
    let mut buf = vec![0u32; win_w * win_h];

    while window.is_open() && running.load(Ordering::Relaxed) {
        // キー入力
        if let Ok(mut keys) = held_keys.lock() {
            keys.clear();
            for &(k, a) in KEY_MAP {
                if window.is_key_down(k) { keys.insert(a.to_string()); }
            }
        }

        // 背景色
        let [br,bg,bb,_] = bg_color.lock().map(|b|*b).unwrap_or([0.05,0.05,0.1,1.0]);
        buf.fill(pack_rgb(br, bg, bb));

        // 描画コマンド
        let cmds = draw_cmds.lock().map(|q|q.clone()).unwrap_or_default();
        for cmd in &cmds { dispatch(&mut buf, win_w, win_h, ssaa, cmd); }

        if window.update_with_buffer(&buf, win_w, win_h).is_err() { break; }
    }
    running.store(false, Ordering::Relaxed);
}

// ── システム DPI ──────────────────────────────────────────────────

#[cfg(target_os="windows")]
fn system_dpi_scale() -> f32 {
    use core::ffi::c_void;
    #[link(name="user32")] extern "system" {
        fn GetDC(h:*const c_void)->*const c_void;
        fn ReleaseDC(h:*const c_void,dc:*const c_void)->i32;
    }
    #[link(name="gdi32")] extern "system" {
        fn GetDeviceCaps(dc:*const c_void,idx:i32)->i32;
    }
    unsafe {
        let dc=GetDC(core::ptr::null()); if dc.is_null(){return 1.0;}
        let d=GetDeviceCaps(dc,88); ReleaseDC(core::ptr::null(),dc);
        (d as f32/96.0).max(1.0)
    }
}
#[cfg(not(target_os="windows"))]
fn system_dpi_scale() -> f32 { 1.0 }

// ── 色 ────────────────────────────────────────────────────────────

#[inline] fn pack_rgb(r:f32,g:f32,b:f32)->u32 {
    ((r.clamp(0.,1.)*255.)as u32)<<16
    |((g.clamp(0.,1.)*255.)as u32)<<8
    |(b.clamp(0.,1.)*255.)as u32
}
#[inline] fn col(c:&[f32;4])->u32 { pack_rgb(c[0],c[1],c[2]) }

/// ハードウェアフレンドリーなアルファブレンド（整数演算）
#[inline]
fn blend(dst:u32, src:u32, a:f32) -> u32 {
    let a  = (a.clamp(0.,1.) * 256.) as u32;
    let ia = 256 - a;
    let r = (((src>>16)&0xFF)*a + ((dst>>16)&0xFF)*ia) >> 8;
    let g = (((src>> 8)&0xFF)*a + ((dst>> 8)&0xFF)*ia) >> 8;
    let b = ((src      &0xFF)*a + (dst      &0xFF)*ia) >> 8;
    (r<<16)|(g<<8)|b
}

// ── ディスパッチ ─────────────────────────────────────────────────

fn dispatch(buf:&mut[u32], w:usize, h:usize, ssaa:usize, cmd:&DrawCmd) {
    match cmd {
        DrawCmd::Circle  {x,y,r,color}         => circle(buf,w,h,*x,*y,*r,col(color),ssaa),
        DrawCmd::Rect    {x,y,w:rw,h:rh,color} => fill_rect(buf,w,h,*x,*y,*rw,*rh,col(color)),
        DrawCmd::Square  {x,y,s,color}         => fill_rect(buf,w,h,*x,*y,*s,*s,col(color)),
        DrawCmd::Line    {x1,y1,x2,y2,color}   => wu_line(buf,w,h,*x1,*y1,*x2,*y2,col(color)),
        DrawCmd::Triangle{x,y,s,color} => {
            let hs = s*0.866_f32;
            polygon(buf,w,h,&[
                (*x,           *y - hs*0.667),
                (*x + s*0.5,  *y + hs*0.333),
                (*x - s*0.5,  *y + hs*0.333),
            ],col(color));
        }
        DrawCmd::Polygon {x,y,s,sides,color} => {
            let n=(*sides).max(3) as usize;
            let pts:Vec<(f32,f32)>=(0..n).map(|i|{
                let a=std::f32::consts::TAU*i as f32/n as f32-std::f32::consts::FRAC_PI_2;
                (*x+s*a.cos(), *y+s*a.sin())
            }).collect();
            polygon(buf,w,h,&pts,col(color));
        }
        DrawCmd::Diamond {x,y,s,color} =>
            polygon(buf,w,h,&[
                (*x,    *y-s), (*x+s, *y),
                (*x,    *y+s), (*x-s, *y),
            ],col(color)),
        DrawCmd::Text{..} | DrawCmd::Background(_) => {}
    }
}

// ── ① 円：エッジゾーン限定SSAA ──────────────────────────────────
//
// 処理コスト:
//   内側ピクセル（d < r-margin）: 直接fill  O(r²)
//   エッジピクセル（|d-r| < margin）: ssaa² サンプル  O(r × ssaa²)
//   外側ピクセル: スキップ
//
// ssaa=4, r=32 の場合:
//   エッジ ~200px × 16 samples = 3,200 ops  (旧SSAA: 2.76M ops)

fn circle(buf:&mut[u32], w:usize, h:usize,
          cx:f32, cy:f32, r:f32, c:u32, ssaa:usize) {
    if r <= 0. { return; }
    let iw=w as i32; let ih=h as i32;

    // エッジゾーン幅（1サンプル間隔 + 余裕 0.5px）
    let margin  = 0.5 + 1.0 / ssaa as f32;
    let r_in    = (r - margin).max(0.);
    let r_out   = r + margin;
    let r_sq    = r   * r;
    let r_in_sq = r_in * r_in;
    let r_out_sq= r_out* r_out;

    let x0 = ((cx - r_out).max(0.)  as i32).min(iw-1);
    let x1 = ((cx + r_out) as i32+1).min(iw);
    let y0 = ((cy - r_out).max(0.)  as i32).min(ih-1);
    let y1 = ((cy + r_out) as i32+1).min(ih);

    let sf   = ssaa as f32;
    let step = 1.0 / sf;
    let inv_n= 1.0 / (sf * sf);

    for py in y0..y1 {
        let dy  = py as f32 + 0.5 - cy;
        let dy2 = dy * dy;
        if dy2 > r_out_sq { continue; }   // 行まるごとスキップ
        let base = py as usize * w;
        for px in x0..x1 {
            let dx  = px as f32 + 0.5 - cx;
            let d2  = dx*dx + dy2;
            if d2 > r_out_sq { continue; }  // 完全外側
            let idx = base + px as usize;
            if d2 < r_in_sq {
                buf[idx] = c;              // 完全内側：直接fill
            } else {
                // エッジゾーン：ssaa²サンプル（sqrtなし！比較のみ）
                let mut hit = 0u32;
                for sy in 0..ssaa {
                    let fy = py as f32 + (sy as f32 + 0.5)*step - cy;
                    let fy2= fy*fy;
                    for sx in 0..ssaa {
                        let fx = px as f32 + (sx as f32 + 0.5)*step - cx;
                        if fx*fx + fy2 <= r_sq { hit += 1; }
                    }
                }
                let alpha = hit as f32 * inv_n;
                if alpha > 0. {
                    buf[idx] = if alpha >= 1. { c } else { blend(buf[idx],c,alpha) };
                }
            }
        }
    }
}

// ── ② Xiaolin Wu 直線 ────────────────────────────────────────────

fn wu_line(buf:&mut[u32], w:usize, h:usize,
           x0:f32,y0:f32,x1:f32,y1:f32, c:u32) {
    let steep=(y1-y0).abs()>(x1-x0).abs();
    let (mut ax,mut ay,mut bx,mut by)=if steep{(y0,x0,y1,x1)}else{(x0,y0,x1,y1)};
    if ax>bx { std::mem::swap(&mut ax,&mut bx); std::mem::swap(&mut ay,&mut by); }
    let dx=bx-ax; let dy=by-ay;
    let grad=if dx.abs()<1e-6{1.}else{dy/dx};
    let iw=w as i32; let ih=h as i32;
    let mut plot=|buf:&mut[u32],xi:i32,yi:i32,br:f32|{
        let (px,py)=if steep{(yi,xi)}else{(xi,yi)};
        if px>=0&&px<iw&&py>=0&&py<ih{
            let idx=py as usize*w+px as usize;
            buf[idx]=blend(buf[idx],c,br.clamp(0.,1.));
        }
    };
    let mut y=ay;
    for xi in ax.round()as i32..=bx.round()as i32 {
        let fpart=y-y.floor();
        plot(buf,xi,y as i32,    1.-fpart);
        plot(buf,xi,y as i32+1, fpart);
        y+=grad;
    }
}

// ── ③ ポリゴン：解析的スキャンラインAA ──────────────────────────
//
// エッジの左右最初・最後のピクセルだけfloatカバレッジでブレンド。
// 内側はsolidフィル。O(perimeter + area)。

fn polygon(buf:&mut[u32], w:usize, h:usize, pts:&[(f32,f32)], c:u32) {
    if pts.len()<3 { return; }
    let iw=w as i32; let ih=h as i32;
    let min_y=pts.iter().map(|p|p.1).fold(f32::INFINITY,f32::min).max(0.)as i32;
    let max_y=pts.iter().map(|p|p.1).fold(f32::NEG_INFINITY,f32::max)
              .min(ih as f32-1.)as i32;
    let n=pts.len();
    for yi in min_y..=max_y {
        let yf=yi as f32+0.5;
        let mut xs:Vec<f32>=Vec::new();
        for i in 0..n {
            let (x1,y1)=pts[i]; let (x2,y2)=pts[(i+1)%n];
            if (y1<=yf&&yf<y2)||(y2<=yf&&yf<y1){
                xs.push(x1+(yf-y1)*(x2-x1)/(y2-y1));
            }
        }
        xs.sort_unstable_by(|a,b|a.partial_cmp(b).unwrap());
        let row=yi as usize*w;
        let mut i=0;
        while i+1<xs.len() {
            let lx=xs[i]; let rx=xs[i+1];
            if rx<0.||lx>=iw as f32 { i+=2; continue; }
            // 左エッジブレンド
            let lf=lx.floor()as i32;
            if lf>=0&&lf<iw {
                let a=(1.-(lx-lx.floor())).clamp(0.,1.);
                let idx=row+lf as usize;
                buf[idx]=blend(buf[idx],c,a);
            }
            // ソリッドフィル（内側）
            let xa=lx.ceil().max(0.)as usize;
            let xb=rx.floor().max(0.).min(iw as f32-1.)as usize;
            if xa<=xb&&row+xb<buf.len() { buf[row+xa..=row+xb].fill(c); }
            // 右エッジブレンド
            let rf=rx.floor()as i32;
            if rf>lf&&rf>=0&&rf<iw {
                let a=(rx-rx.floor()).clamp(0.,1.);
                if a>0. {
                    let idx=row+rf as usize;
                    buf[idx]=blend(buf[idx],c,a);
                }
            }
            i+=2;
        }
    }
}

// ── ④ 矩形（AA不要・高速fill）────────────────────────────────────

fn fill_rect(buf:&mut[u32], w:usize, h:usize,
             x:f32, y:f32, rw:f32, rh:f32, c:u32) {
    let x0=(x.max(0.)      as usize).min(w);
    let y0=(y.max(0.)      as usize).min(h);
    let x1=((x+rw).max(0.) as usize).min(w);
    let y1=((y+rh).max(0.) as usize).min(h);
    if x0>=x1||y0>=y1 { return; }
    for py in y0..y1 { buf[py*w+x0..py*w+x1].fill(c); }
}
