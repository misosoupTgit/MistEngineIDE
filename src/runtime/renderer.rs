/// Immediate Mode レンダラー
/// draw.circle / draw.rect / draw.line 等の描画命令を管理

#[derive(Debug, Clone, Copy)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const RED:         Color = Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const GREEN:       Color = Color { r: 0.0, g: 1.0, b: 0.0, a: 1.0 };
    pub const BLUE:        Color = Color { r: 0.0, g: 0.0, b: 1.0, a: 1.0 };
    pub const WHITE:       Color = Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
    pub const BLACK:       Color = Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const TRANSPARENT: Color = Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };

    pub fn new(r: f32, g: f32, b: f32, a: f32) -> Self { Color { r, g, b, a } }

    pub fn from_hex(hex: &str) -> Self {
        let hex = hex.trim_start_matches('#');
        let (r, g, b) = if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0) as f32 / 255.0;
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0) as f32 / 255.0;
            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0) as f32 / 255.0;
            (r, g, b)
        } else { (0.0, 0.0, 0.0) };
        Color { r, g, b, a: 1.0 }
    }

    pub fn lerp(c1: Color, c2: Color, t: f32) -> Self {
        Color {
            r: c1.r + (c2.r - c1.r) * t,
            g: c1.g + (c2.g - c1.g) * t,
            b: c1.b + (c2.b - c1.b) * t,
            a: c1.a + (c2.a - c1.a) * t,
        }
    }
}

/// 描画コマンド（Immediate Mode）
#[derive(Debug, Clone)]
pub enum DrawCommand {
    Circle {
        x: f32, y: f32, size: f32,
        skew: Option<f32>,
        color: Color,
    },
    Square {
        x: f32, y: f32, size: f32,
        angle: Option<f32>,
        color: Color,
    },
    Triangle {
        x: f32, y: f32, size: f32,
        angle: Option<f32>,
        color: Color,
    },
    Polygon {
        x: f32, y: f32, size: f32,
        sides: u32,
        color: Color,
    },
    Diamond {
        x: f32, y: f32, size: f32,
        color: Color,
    },
    Rect {
        x: f32, y: f32, width: f32, height: f32,
        color: Color,
    },
    Line {
        x1: f32, y1: f32, x2: f32, y2: f32,
        thickness: f32,
        color: Color,
    },
    Image {
        path: String,
        x: f32, y: f32,
        scale: f32, angle: f32,
    },
}

pub struct Renderer {
    pub commands: Vec<DrawCommand>,
    pub background_color: Color,
    pub width: u32,
    pub height: u32,
}

impl Renderer {
    pub fn new(width: u32, height: u32) -> Self {
        Renderer {
            commands: Vec::new(),
            background_color: Color::BLACK,
            width,
            height,
        }
    }

    pub fn begin_frame(&mut self) {
        self.commands.clear();
    }

    pub fn submit(&mut self, cmd: DrawCommand) {
        self.commands.push(cmd);
    }

    // ──────────────────────────────
    // Mistral draw.* API
    // ──────────────────────────────

    pub fn circle(&mut self, x: f32, y: f32, size: f32, color: Color, skew: Option<f32>) {
        self.submit(DrawCommand::Circle { x, y, size, skew, color });
    }

    pub fn square(&mut self, x: f32, y: f32, size: f32, color: Color, angle: Option<f32>) {
        self.submit(DrawCommand::Square { x, y, size, angle, color });
    }

    pub fn triangle(&mut self, x: f32, y: f32, size: f32, color: Color, angle: Option<f32>) {
        self.submit(DrawCommand::Triangle { x, y, size, angle, color });
    }

    pub fn polygon(&mut self, x: f32, y: f32, size: f32, sides: u32, color: Color) {
        self.submit(DrawCommand::Polygon { x, y, size, sides, color });
    }

    pub fn diamond(&mut self, x: f32, y: f32, size: f32, color: Color) {
        self.submit(DrawCommand::Diamond { x, y, size, color });
    }

    pub fn rect(&mut self, x: f32, y: f32, width: f32, height: f32, color: Color) {
        self.submit(DrawCommand::Rect { x, y, width, height, color });
    }

    pub fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, thickness: f32, color: Color) {
        self.submit(DrawCommand::Line { x1, y1, x2, y2, thickness, color });
    }

    pub fn image(&mut self, path: &str, x: f32, y: f32, scale: f32, angle: f32) {
        self.submit(DrawCommand::Image { path: path.to_string(), x, y, scale, angle });
    }
}
