use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::collections::HashSet;

use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowAttributes},
};
use wgpu::util::DeviceExt;
use lyon::tessellation::*;
use lyon::path::Path;
use lyon::math::point;

use crate::runtime::vm::{DrawCmd, GameState, Interpreter, Value};

pub struct GameWindowConfig {
    pub title:      String,
    pub width:      u32,
    pub height:     u32,
    pub high_dpi:   bool,
    pub anti_alias: f32,
    pub resizable:  bool,
    pub vsync:      bool,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex { pos: [f32; 2], col: [f32; 4] }

#[inline]
fn ndc(x: f32, y: f32, w: u32, h: u32) -> [f32; 2] {
    [x * 2.0 / w as f32 - 1.0, 1.0 - y * 2.0 / h as f32]
}

const SHADER_SRC: &str = r#"
struct VIn  { @location(0) pos: vec2<f32>, @location(1) col: vec4<f32> }
struct VOut { @builtin(position) pos: vec4<f32>, @location(0) col: vec4<f32> }
@vertex
fn vs(in: VIn) -> VOut { return VOut(vec4<f32>(in.pos, 0.0, 1.0), in.col); }
@fragment
fn fs(in: VOut) -> @location(0) vec4<f32> { return in.col; }
"#;

struct GpuState {
    window:      Arc<Window>,
    surface:     wgpu::Surface<'static>,
    device:      wgpu::Device,
    queue:       wgpu::Queue,
    surf_config: wgpu::SurfaceConfiguration,
    pipeline:    wgpu::RenderPipeline,
}

impl GpuState {
    fn new(window: Arc<Window>, vsync: bool) -> Self {
        pollster::block_on(Self::init(window, vsync))
    }
    async fn init(window: Arc<Window>, vsync: bool) -> Self {
        let size     = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface  = instance.create_surface(window.clone()).unwrap();
        let adapter  = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                power_preference: wgpu::PowerPreference::HighPerformance,
                ..Default::default()
            })
            .await.expect("wgpu adapter failed");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await.expect("wgpu device failed");
        let caps = surface.get_capabilities(&adapter);
        let fmt  = caps.formats[0];
        let pm   = if vsync { wgpu::PresentMode::AutoVsync } else { wgpu::PresentMode::AutoNoVsync };
        let surf_config = wgpu::SurfaceConfiguration {
            usage:   wgpu::TextureUsages::RENDER_ATTACHMENT,
            format:  fmt,
            width:   size.width.max(1),
            height:  size.height.max(1),
            present_mode: pm,
            alpha_mode:   caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surf_config);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
        });
        let vbl = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode:    wgpu::VertexStepMode::Vertex,
            attributes:   &[
                wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 8, shader_location: 1, format: wgpu::VertexFormat::Float32x4 },
            ],
        };
        let layout   = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor::default());
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  None,
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader, entry_point: "vs", buffers: &[vbl],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader, entry_point: "fs",
                targets: &[Some(wgpu::ColorTargetState {
                    format:     fmt,
                    blend:      Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive:    wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() },
            depth_stencil: None,
            multisample:   wgpu::MultisampleState::default(),
            multiview:     None,
        });
        GpuState { window, surface, device, queue, surf_config, pipeline }
    }
    fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 { return; }
        self.surf_config.width  = w;
        self.surf_config.height = h;
        self.surface.configure(&self.device, &self.surf_config);
    }
    fn render(&mut self, cmds: &[DrawCmd], bg: [f32; 4]) {
        let sw = self.surf_config.width;
        let sh = self.surf_config.height;
        let mut verts:   Vec<Vertex> = Vec::new();
        let mut indices: Vec<u32>    = Vec::new();
        let mut fill   = FillTessellator::new();
        let mut stroke = StrokeTessellator::new();
        for cmd in cmds {
            match cmd {
                DrawCmd::Background(_) | DrawCmd::Text { .. } | DrawCmd::Image { .. } => {}
                _ => tess_cmd(cmd, &mut fill, &mut stroke, &mut verts, &mut indices, sw, sh),
            }
        }
        let frame = match self.surface.get_current_texture() {
            Ok(f)  => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.surf_config);
                return;
            }
            Err(_) => return,
        };
        let view = frame.texture.create_view(&Default::default());
        let mut enc = self.device.create_command_encoder(&Default::default());
        // バッファはパスより長く生きる必要があるため外側で作成
        let vbuf = if !verts.is_empty() {
            Some(self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None, contents: bytemuck::cast_slice(&verts),
                usage: wgpu::BufferUsages::VERTEX,
            }))
        } else { None };
        let ibuf = if !indices.is_empty() {
            Some(self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None, contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            }))
        } else { None };
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view, resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: bg[0] as f64, g: bg[1] as f64,
                            b: bg[2] as f64, a: bg[3] as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes:         None,
                occlusion_query_set:      None,
            });
            if let (Some(vb), Some(ib)) = (&vbuf, &ibuf) {
                pass.set_pipeline(&self.pipeline);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
            }
        }
        self.queue.submit([enc.finish()]);
        frame.present();
    }
}

fn tess_cmd(
    cmd: &DrawCmd, fill: &mut FillTessellator, stroke: &mut StrokeTessellator,
    verts: &mut Vec<Vertex>, indices: &mut Vec<u32>, sw: u32, sh: u32,
) {
    match cmd {
        DrawCmd::Circle { x, y, r, color } => {
            let base = verts.len() as u32;
            let mut geo: VertexBuffers<[f32;2], u32> = VertexBuffers::new();
            fill.tessellate_circle(
                point(*x, *y), *r, &FillOptions::default(),
                &mut BuffersBuilder::new(&mut geo, |p: FillVertex| p.position().to_array()),
            ).ok();
            let col = *color;
            for p in &geo.vertices { verts.push(Vertex { pos: ndc(p[0],p[1],sw,sh), col }); }
            for i in &geo.indices  { indices.push(base + i); }
        }
        DrawCmd::Rect { x, y, w, h, color } => {
            let base = verts.len() as u32;
            let mut geo: VertexBuffers<[f32;2], u32> = VertexBuffers::new();
            let rect = lyon::math::Box2D::new(point(*x,*y), point(x+w, y+h));
            fill.tessellate_rectangle(&rect, &FillOptions::default(),
                &mut BuffersBuilder::new(&mut geo, |p: FillVertex| p.position().to_array()),
            ).ok();
            let col = *color;
            for p in &geo.vertices { verts.push(Vertex { pos: ndc(p[0],p[1],sw,sh), col }); }
            for i in &geo.indices  { indices.push(base + i); }
        }
        DrawCmd::Square { x, y, s, color } => {
            let base = verts.len() as u32;
            let mut geo: VertexBuffers<[f32;2], u32> = VertexBuffers::new();
            let rect = lyon::math::Box2D::new(point(*x,*y), point(x+s, y+s));
            fill.tessellate_rectangle(&rect, &FillOptions::default(),
                &mut BuffersBuilder::new(&mut geo, |p: FillVertex| p.position().to_array()),
            ).ok();
            let col = *color;
            for p in &geo.vertices { verts.push(Vertex { pos: ndc(p[0],p[1],sw,sh), col }); }
            for i in &geo.indices  { indices.push(base + i); }
        }
        DrawCmd::Line { x1, y1, x2, y2, color } => {
            let base = verts.len() as u32;
            let mut geo: VertexBuffers<[f32;2], u32> = VertexBuffers::new();
            let mut b = Path::builder();
            b.begin(point(*x1,*y1));
            b.line_to(point(*x2,*y2));
            b.end(false);
            stroke.tessellate_path(
                &b.build(), &StrokeOptions::default().with_line_width(2.0),
                &mut BuffersBuilder::new(&mut geo, |p: StrokeVertex| p.position().to_array()),
            ).ok();
            let col = *color;
            for p in &geo.vertices { verts.push(Vertex { pos: ndc(p[0],p[1],sw,sh), col }); }
            for i in &geo.indices  { indices.push(base + i); }
        }
        DrawCmd::Triangle { x, y, s, color } => {
            let hs  = s * 0.866_f32;
            let p0  = ndc(*x,         y - hs*0.667, sw, sh);
            let p1  = ndc(x + s*0.5,  y + hs*0.333, sw, sh);
            let p2  = ndc(x - s*0.5,  y + hs*0.333, sw, sh);
            let col = *color;
            let base = verts.len() as u32;
            verts.extend_from_slice(&[
                Vertex { pos: p0, col }, Vertex { pos: p1, col }, Vertex { pos: p2, col },
            ]);
            indices.extend_from_slice(&[base, base+1, base+2]);
        }
        DrawCmd::Polygon { x, y, s, sides, color } => {
            let n    = (*sides).max(3) as u32;
            let base = verts.len() as u32;
            let mut geo: VertexBuffers<[f32;2], u32> = VertexBuffers::new();
            let mut b = Path::builder();
            for i in 0..n {
                let a  = std::f32::consts::TAU * i as f32 / n as f32 - std::f32::consts::FRAC_PI_2;
                let px = x + s * a.cos();
                let py = y + s * a.sin();
                if i == 0 { b.begin(point(px,py)); } else { b.line_to(point(px,py)); }
            }
            b.end(true);
            fill.tessellate_path(&b.build(), &FillOptions::default(),
                &mut BuffersBuilder::new(&mut geo, |p: FillVertex| p.position().to_array()),
            ).ok();
            let col = *color;
            for p in &geo.vertices { verts.push(Vertex { pos: ndc(p[0],p[1],sw,sh), col }); }
            for i in &geo.indices  { indices.push(base + i); }
        }
        DrawCmd::Diamond { x, y, s, color } => {
            let p0 = ndc(*x,    y - s, sw, sh);
            let p1 = ndc(x + s, *y,    sw, sh);
            let p2 = ndc(*x,    y + s, sw, sh);
            let p3 = ndc(x - s, *y,    sw, sh);
            let col  = *color;
            let base = verts.len() as u32;
            verts.extend_from_slice(&[
                Vertex { pos: p0, col }, Vertex { pos: p1, col },
                Vertex { pos: p2, col }, Vertex { pos: p3, col },
            ]);
            indices.extend_from_slice(&[base,base+1,base+2, base,base+2,base+3]);
        }
        _ => {}
    }
}

fn key_to_action(k: KeyCode) -> Option<&'static str> {
    Some(match k {
        KeyCode::ArrowUp    | KeyCode::KeyW => "up",
        KeyCode::ArrowDown  | KeyCode::KeyS => "down",
        KeyCode::ArrowLeft  | KeyCode::KeyA => "left",
        KeyCode::ArrowRight | KeyCode::KeyD => "right",
        KeyCode::Space                       => "jump",
        KeyCode::KeyZ                        => "attack",
        KeyCode::KeyX                        => "action",
        KeyCode::Enter                       => "confirm",
        KeyCode::Escape                      => "cancel",
        KeyCode::ShiftLeft | KeyCode::ShiftRight => "shift",
        _ => return None,
    })
}

struct GameApp {
    win_cfg:    GameWindowConfig,
    interp:     Interpreter,
    state:      GameState,
    gpu:        Option<GpuState>,
    last_time:  std::time::Instant,
    fps_accum:  f64,
    fps_frames: u32,
    held_keys:  HashSet<String>,
    ready_done: bool,
}

impl ApplicationHandler for GameApp {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gpu.is_some() { return; }
        let attrs = WindowAttributes::default()
            .with_title(&self.win_cfg.title)
            .with_inner_size(winit::dpi::PhysicalSize::new(self.win_cfg.width, self.win_cfg.height))
            .with_resizable(self.win_cfg.resizable);
        let window = Arc::new(el.create_window(attrs).expect("window create failed"));
        self.state.screen_w.store(self.win_cfg.width,  Ordering::Relaxed);
        self.state.screen_h.store(self.win_cfg.height, Ordering::Relaxed);
        if !self.ready_done {
            self.ready_done = true;
            if let Some(func) = self.interp.get_var("ready") {
                if let Err(e) = self.interp.call_value(func, vec![]) {
                    eprintln!("[Mistral] ready() error: {}", e);
                }
            }
        }
        self.gpu = Some(GpuState::new(window, self.win_cfg.vsync));
        self.last_time = std::time::Instant::now();
    }
    fn window_event(&mut self, el: &ActiveEventLoop, _wid: winit::window::WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.state.running.store(false, Ordering::Relaxed);
                el.exit();
            }
            WindowEvent::Resized(sz) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(sz.width, sz.height);
                    self.state.screen_w.store(sz.width,  Ordering::Relaxed);
                    self.state.screen_h.store(sz.height, Ordering::Relaxed);
                }
            }
            WindowEvent::KeyboardInput {
                event: KeyEvent { physical_key: PhysicalKey::Code(kc), state: ks, .. }, ..
            } => {
                if let Some(action) = key_to_action(kc) {
                    match ks {
                        ElementState::Pressed  => { self.held_keys.insert(action.to_string()); }
                        ElementState::Released => { self.held_keys.remove(action); }
                    }
                    if let Ok(mut held) = self.state.held_keys.lock() {
                        *held = self.held_keys.clone();
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if !self.state.running.load(Ordering::Relaxed) { el.exit(); return; }
                let now = std::time::Instant::now();
                let dt  = now.duration_since(self.last_time).as_secs_f64();
                self.last_time = now;
                self.fps_accum  += dt;
                self.fps_frames += 1;
                if self.fps_accum >= 0.5 {
                    let fps = (self.fps_frames as f64 / self.fps_accum).round() as u32;
                    self.state.fps.store(fps, Ordering::Relaxed);
                    self.fps_accum  = 0.0;
                    self.fps_frames = 0;
                }
                if let Ok(mut q) = self.state.console.try_lock() {
                    for line in q.drain(..) { println!("{}", line); }
                }
                if let Some(func) = self.interp.get_var("update") {
                    if let Err(e) = self.interp.call_value(func, vec![Value::Float(dt)]) {
                        eprintln!("[Mistral] update() error: {}", e);
                        self.state.running.store(false, Ordering::Relaxed);
                        el.exit();
                        return;
                    }
                }
                self.interp.frame_cmds.clear();
                if let Some(func) = self.interp.get_var("draw") {
                    if let Err(e) = self.interp.call_value(func, vec![]) {
                        eprintln!("[Mistral] draw() error: {}", e);
                        self.state.running.store(false, Ordering::Relaxed);
                        el.exit();
                        return;
                    }
                }
                let mut bg = [0.05_f32, 0.05, 0.1, 1.0];
                for cmd in &self.interp.frame_cmds {
                    if let DrawCmd::Background(c) = cmd { bg = *c; break; }
                }
                if let Some(gpu) = &mut self.gpu {
                    gpu.render(&self.interp.frame_cmds, bg);
                    gpu.window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

pub fn run_game_window(config: GameWindowConfig, interp: Interpreter, state: GameState) {
    let el = EventLoop::new().expect("EventLoop failed");
    el.set_control_flow(ControlFlow::Poll);
    let mut app = GameApp {
        win_cfg: config, interp, state, gpu: None,
        last_time: std::time::Instant::now(),
        fps_accum: 0.0, fps_frames: 0,
        held_keys: HashSet::new(), ready_done: false,
    };
    el.run_app(&mut app).expect("event loop failed");
}