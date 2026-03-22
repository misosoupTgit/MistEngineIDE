use egui::Color32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThemeKind { Dark, White, Legacy, DarkBlue }

impl ThemeKind {
    pub fn all() -> &'static [ThemeKind] {
        &[ThemeKind::Dark, ThemeKind::White, ThemeKind::Legacy, ThemeKind::DarkBlue]
    }
    pub fn name(self) -> &'static str {
        match self {
            ThemeKind::Dark     => "Dark",
            ThemeKind::White    => "Light",
            ThemeKind::Legacy   => "Legacy",
            ThemeKind::DarkBlue => "Dark Blue",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub kind: ThemeKind,
    pub bg: Color32,
    pub panel_bg: Color32,
    pub editor_bg: Color32,
    pub gutter_bg: Color32,
    pub active_line: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub text_accent: Color32,
    pub border: Color32,
    pub accent: Color32,
    pub button_bg: Color32,
    pub button_hover: Color32,
    pub run_btn: Color32,
    pub build_btn: Color32,
    pub line_number: Color32,
    pub cursor: Color32,
    pub selection: Color32,
    // syntax
    pub syn_keyword:  Color32,
    pub syn_type:     Color32,
    pub syn_builtin:  Color32,
    pub syn_func:     Color32,
    pub syn_variable: Color32,
    pub syn_string:   Color32,
    pub syn_number:   Color32,
    pub syn_comment:  Color32,
    pub syn_operator: Color32,
    pub syn_literal:  Color32,
    // console
    pub con_normal: Color32,
    pub con_debug:  Color32,
    pub con_warn:   Color32,
    pub con_error:  Color32,
}

impl Theme {
    pub fn from_kind(k: ThemeKind) -> Self {
        match k {
            ThemeKind::Dark => Self {
                kind: k,
                bg:          Color32::from_rgb(24, 24, 27),
                panel_bg:    Color32::from_rgb(30, 30, 36),
                editor_bg:   Color32::from_rgb(18, 18, 22),
                gutter_bg:   Color32::from_rgb(22, 22, 28),
                active_line: Color32::from_rgba_premultiplied(255,255,255,12),
                text:        Color32::from_rgb(212,212,212),
                text_muted:  Color32::from_rgb(110,110,125),
                text_accent: Color32::from_rgb(150,200,255),
                border:      Color32::from_rgb(48,48,60),
                accent:      Color32::from_rgb(80,120,220),
                button_bg:   Color32::from_rgb(42,42,55),
                button_hover:Color32::from_rgb(55,55,72),
                run_btn:     Color32::from_rgb(46,160,78),
                build_btn:   Color32::from_rgb(55,80,180),
                line_number: Color32::from_rgb(75,75,95),
                cursor:      Color32::from_rgb(150,200,255),
                selection:   Color32::from_rgba_premultiplied(80,120,220,80),
                syn_keyword:  Color32::from_rgb(200,120,255),
                syn_type:     Color32::from_rgb(100,180,255),
                syn_builtin:  Color32::from_rgb(100,220,200),
                syn_func:     Color32::from_rgb(220,200,120),
                syn_variable: Color32::from_rgb(180,220,255),
                syn_string:   Color32::from_rgb(160,210,120),
                syn_number:   Color32::from_rgb(255,170,100),
                syn_comment:  Color32::from_rgb(94,130,92),
                syn_operator: Color32::from_rgb(200,200,200),
                syn_literal:  Color32::from_rgb(255,150,100),
                con_normal:  Color32::from_rgb(200,200,200),
                con_debug:   Color32::from_rgb(100,180,255),
                con_warn:    Color32::from_rgb(255,200,80),
                con_error:   Color32::from_rgb(255,100,100),
            },
            ThemeKind::White => Self {
                kind: k,
                bg:          Color32::from_rgb(245,245,250),
                panel_bg:    Color32::from_rgb(235,235,242),
                editor_bg:   Color32::WHITE,
                gutter_bg:   Color32::from_rgb(238,238,246),
                active_line: Color32::from_rgba_premultiplied(0,0,0,12),
                text:        Color32::from_rgb(30,30,40),
                text_muted:  Color32::from_rgb(120,120,145),
                text_accent: Color32::from_rgb(40,80,200),
                border:      Color32::from_rgb(210,210,225),
                accent:      Color32::from_rgb(60,100,220),
                button_bg:   Color32::from_rgb(218,218,232),
                button_hover:Color32::from_rgb(200,200,220),
                run_btn:     Color32::from_rgb(38,150,68),
                build_btn:   Color32::from_rgb(50,70,180),
                line_number: Color32::from_rgb(160,160,182),
                cursor:      Color32::from_rgb(40,80,200),
                selection:   Color32::from_rgba_premultiplied(60,100,220,60),
                syn_keyword:  Color32::from_rgb(120,0,190),
                syn_type:     Color32::from_rgb(20,100,180),
                syn_builtin:  Color32::from_rgb(0,130,120),
                syn_func:     Color32::from_rgb(140,100,0),
                syn_variable: Color32::from_rgb(30,60,130),
                syn_string:   Color32::from_rgb(40,140,20),
                syn_number:   Color32::from_rgb(180,80,0),
                syn_comment:  Color32::from_rgb(90,120,80),
                syn_operator: Color32::from_rgb(80,80,100),
                syn_literal:  Color32::from_rgb(180,80,0),
                con_normal:  Color32::from_rgb(40,40,50),
                con_debug:   Color32::from_rgb(20,80,180),
                con_warn:    Color32::from_rgb(140,90,0),
                con_error:   Color32::from_rgb(200,0,0),
            },
            ThemeKind::Legacy => Self {
                kind: k,
                bg:          Color32::from_rgb(40,40,40),
                panel_bg:    Color32::from_rgb(50,50,50),
                editor_bg:   Color32::from_rgb(35,35,35),
                gutter_bg:   Color32::from_rgb(40,40,40),
                active_line: Color32::from_rgba_premultiplied(255,255,255,15),
                text:        Color32::from_rgb(200,200,200),
                text_muted:  Color32::from_rgb(130,130,130),
                text_accent: Color32::from_rgb(180,200,220),
                border:      Color32::from_rgb(60,60,60),
                accent:      Color32::from_rgb(100,140,200),
                button_bg:   Color32::from_rgb(55,55,55),
                button_hover:Color32::from_rgb(70,70,70),
                run_btn:     Color32::from_rgb(55,145,75),
                build_btn:   Color32::from_rgb(65,85,160),
                line_number: Color32::from_rgb(90,90,110),
                cursor:      Color32::from_rgb(180,200,220),
                selection:   Color32::from_rgba_premultiplied(100,140,200,70),
                syn_keyword:  Color32::from_rgb(180,100,200),
                syn_type:     Color32::from_rgb(100,160,220),
                syn_builtin:  Color32::from_rgb(100,200,180),
                syn_func:     Color32::from_rgb(200,180,100),
                syn_variable: Color32::from_rgb(160,200,220),
                syn_string:   Color32::from_rgb(140,190,100),
                syn_number:   Color32::from_rgb(220,150,80),
                syn_comment:  Color32::from_rgb(90,120,90),
                syn_operator: Color32::from_rgb(180,180,180),
                syn_literal:  Color32::from_rgb(220,130,80),
                con_normal:  Color32::from_rgb(180,180,180),
                con_debug:   Color32::from_rgb(80,160,220),
                con_warn:    Color32::from_rgb(200,180,60),
                con_error:   Color32::from_rgb(220,80,80),
            },
            ThemeKind::DarkBlue => Self {
                kind: k,
                bg:          Color32::from_rgb(10,15,30),
                panel_bg:    Color32::from_rgb(14,20,44),
                editor_bg:   Color32::from_rgb(8,12,24),
                gutter_bg:   Color32::from_rgb(10,15,32),
                active_line: Color32::from_rgba_premultiplied(100,150,255,20),
                text:        Color32::from_rgb(200,215,255),
                text_muted:  Color32::from_rgb(100,120,180),
                text_accent: Color32::from_rgb(120,180,255),
                border:      Color32::from_rgb(28,44,80),
                accent:      Color32::from_rgb(60,120,255),
                button_bg:   Color32::from_rgb(18,28,60),
                button_hover:Color32::from_rgb(28,44,80),
                run_btn:     Color32::from_rgb(38,158,76),
                build_btn:   Color32::from_rgb(38,78,200),
                line_number: Color32::from_rgb(58,78,140),
                cursor:      Color32::from_rgb(120,180,255),
                selection:   Color32::from_rgba_premultiplied(60,120,255,80),
                syn_keyword:  Color32::from_rgb(180,100,255),
                syn_type:     Color32::from_rgb(80,180,255),
                syn_builtin:  Color32::from_rgb(80,215,200),
                syn_func:     Color32::from_rgb(220,190,100),
                syn_variable: Color32::from_rgb(160,200,255),
                syn_string:   Color32::from_rgb(120,200,100),
                syn_number:   Color32::from_rgb(255,158,80),
                syn_comment:  Color32::from_rgb(68,98,118),
                syn_operator: Color32::from_rgb(180,200,255),
                syn_literal:  Color32::from_rgb(255,140,80),
                con_normal:  Color32::from_rgb(180,200,240),
                con_debug:   Color32::from_rgb(80,170,255),
                con_warn:    Color32::from_rgb(255,188,60),
                con_error:   Color32::from_rgb(255,80,80),
            },
        }
    }

    pub fn dark() -> Self { Self::from_kind(ThemeKind::Dark) }

    /// egui Visuals に適用
    pub fn apply(&self, ctx: &egui::Context) {
        let mut v = if self.kind == ThemeKind::White {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        };
        v.panel_fill             = self.panel_bg;
        v.window_fill            = self.bg;
        v.extreme_bg_color       = self.editor_bg;
        v.faint_bg_color         = self.gutter_bg;
        v.selection.bg_fill      = self.selection;
        v.widgets.noninteractive.bg_fill   = self.panel_bg;
        v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, self.border);
        v.widgets.inactive.bg_fill         = self.button_bg;
        v.widgets.hovered.bg_fill          = self.button_hover;
        v.widgets.active.bg_fill           = self.accent;
        v.override_text_color = Some(self.text);
        ctx.set_visuals(v);
    }
}
