        CentralPanel::default()
            .frame(Frame::none().fill(editor_bg))
            .show(ctx, |ui| {
                let mut layouter = move |u: &egui::Ui, text: &str, wrap_width: f32| {
                    let mut job = build_layout_job(text, &theme_clone);
                    job.wrap.max_width = wrap_width;
                    u.fonts(|f| f.layout_job(job))
                };

                // ガターはScrollAreaの外・CentralPanelに直接固定描画
                // TextEditのScrollAreaとは独立しているので横スクロールで動かない
                // 行番号はte.galley_pos（スクリーン座標）を使うので誤差ゼロ
                let gutter_w    = 52.0f32;
                let panel_rect  = ui.available_rect_before_wrap();
                let gutter_rect = Rect::from_min_size(panel_rect.min, vec2(gutter_w, panel_rect.height()));
                let editor_rect = Rect::from_min_max(
                    pos2(panel_rect.min.x + gutter_w, panel_rect.min.y), panel_rect.max,
                );

                // ガター背景（固定）
                ui.painter().rect_filled(gutter_rect, 0.0, gutter_bg);
                ui.painter().line_segment(
                    [gutter_rect.right_top(), gutter_rect.right_bottom()],
                    Stroke::new(1.0, Color32::from_rgb(60, 60, 75)),
                );

                // エディタ（ScrollArea::both）
                let te_resp = ui.allocate_ui_at_rect(editor_rect, |ui| {
                    ScrollArea::both()
                        .id_source("editor_scroll")
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            let te = TextEdit::multiline(&mut self.editor.text)
                                .id(Id::new("code_editor"))
                                .frame(false)
                                .desired_width(f32::INFINITY)
                                .desired_rows(line_count)
                                .code_editor()
                                .layouter(&mut layouter)
                                .show(ui);
                            if te.response.changed() { self.editor.dirty = true; }
                            te
                        })
                });
                let te = te_resp.inner.inner;

                // 行番号描画
                // te.galley_pos はスクリーン座標（スクロール位置込み）
                // → row.rect と組み合わせるだけで完璧に一致
                let galley_pos   = te.galley_pos;
                let clip         = ui.painter().with_clip_rect(gutter_rect);
                let mut line_num = 1usize;
                for row in &te.galley.rows {
                    let row_y = galley_pos.y + row.rect.min.y + row.rect.height() * 0.5;
                    if row_y < gutter_rect.top() - 30.0 {
                        if row.ends_with_newline { line_num += 1; }
                        continue;
                    }
                    if row_y > gutter_rect.bottom() + 30.0 { break; }
                    clip.text(
                        pos2(gutter_rect.right() - 6.0, row_y),
                        Align2::RIGHT_CENTER,
                        format!("{}", line_num),
                        FontId::monospace(13.0),
                        ln_color,
                    );
                    if row.ends_with_newline { line_num += 1; }
                }

                // コード補完
                if te.response.has_focus() {
                    if let Some(cr) = &te.cursor_range {
                        let bp = char_to_byte(&self.editor.text, cr.primary.ccursor.index);
                        if let Some((ws, word)) = word_at(&self.editor.text, bp) {
                            let sug = mistral_suggestions(&word);
                            self.ac_suggestions = sug;
                            self.ac_word_start  = ws;
                            if self.ac_sel >= self.ac_suggestions.len() { self.ac_sel = 0; }
                        } else {
                            self.ac_suggestions.clear();
                        }
                    }
                    if !ac_suggestions.is_empty() {
                        let popup_y = if let Some(cr) = &te.cursor_range {
                            let ri = cr.primary.pcursor.paragraph.min(te.galley.rows.len().saturating_sub(1));
                            te.galley.rows.get(ri)
                                .map(|r| galley_pos.y + r.rect.max.y + 4.0)
                                .unwrap_or(editor_rect.top() + 24.0)
                        } else { editor_rect.top() + 24.0 };

                        egui::Area::new(Id::new("ac_popup"))
                            .order(Order::Foreground)
                            .fixed_pos(pos2(editor_rect.left() + 4.0, popup_y))
                            .show(ui.ctx(), |ui| {
                                Frame::none()
                                    .fill(ac_bg)
                                    .stroke(Stroke::new(1.0, ac_accent))
                                    .rounding(Rounding::same(6.0))
                                    .inner_margin(4.0)
                                    .show(ui, |ui| {
                                        ui.set_max_width(240.0);
                                        for (i, &sug) in ac_suggestions.iter().enumerate() {
                                            let is_sel = i == ac_sel;
                                            let bg     = if is_sel { ac_accent } else { Color32::TRANSPARENT };
                                            let tc     = if is_sel { Color32::WHITE } else { ac_text };
                                            let r = ui.add(
                                                Button::new(RichText::new(sug).color(tc).monospace().size(13.0))
                                                    .fill(bg).frame(true).min_size(vec2(200.0, 22.0))
                                            );
                                            if r.clicked() { ui.ctx().data_mut(|d| { d.insert_temp(Id::new("ac_chosen"), sug.to_string()); }); }
                                            if r.hovered() { ui.ctx().data_mut(|d| { d.insert_temp(Id::new("ac_hi"), i); }); }
                                        }
                                        ui.separator();
                                        ui.label(RichText::new("Tab/Enter=確定  Esc=閉じる").color(ac_muted).size(10.0));
                                    });
                            });
                        if let Some(c) = ui.ctx().data(|d| d.get_temp::<String>(Id::new("ac_chosen"))) {
                            self.ac_insert = Some(c);
                            ui.ctx().data_mut(|d| { d.remove::<String>(Id::new("ac_chosen")); });
                        }
                        if let Some(i) = ui.ctx().data(|d| d.get_temp::<usize>(Id::new("ac_hi"))) {
                            self.ac_sel = i;
                            ui.ctx().data_mut(|d| { d.remove::<usize>(Id::new("ac_hi")); });
                        }
                    }
                } else {
                    self.ac_suggestions.clear();
                }
            });
    }
}

// ── ファイルツリー ──────────────────────────────────

fn render_tree(
    ui: &mut Ui,
    node: &mut FileNode,
    selected: &Option<PathBuf>,
    theme: &Theme,
) -> Option<PathBuf> {
    let mut open_req = None;
    if node.is_dir {
        let resp = CollapsingHeader::new(
            RichText::new(format!("📁 {}", node.name)).color(theme.text_accent)
        )
        .id_source(ui.make_persistent_id(&node.path))
        .default_open(node.expanded)
        .show(ui, |ui| {
            for child in &mut node.children {
                if let Some(p) = render_tree(ui, child, selected, theme) { open_req = Some(p); }
            }
        });
        node.expanded = resp.openness > 0.5;
    } else {
        let is_sel = selected.as_ref().map_or(false, |p| *p == node.path);
        let color  = if node.name.ends_with(".mist") { theme.syn_keyword }
                     else if node.name.ends_with(".json") { theme.syn_string }
                     else { theme.text_muted };
        let icon = if node.name.ends_with(".mist") { "📄" } else { "📃" };
        if ui.add(SelectableLabel::new(is_sel,
            RichText::new(format!("{} {}", icon, node.name)).color(color).size(13.0)
        )).clicked() { open_req = Some(node.path.clone()); }
    }
    open_req
}

// ── eframe::App ─────────────────────────────────────

impl eframe::App for IdeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        match self.screen {
            Screen::ThemeSelect   => self.ui_theme_select(ctx),
            Screen::ProjectSelect => self.ui_project_select(ctx),
            Screen::Ide           => self.ui_ide(ctx),
        }
    }
}

// ── コード補完ヘルパー ────────────────────────────────

fn mistral_suggestions(prefix: &str) -> Vec<&'static str> {
    const KEYWORDS: &[&str] = &[
        "let","func","return","if","ifelse","else","while","for","in",
        "range","switch","case","default","try","catch","import","repeat",
        "clone","as","break","continue","and","or","not","true","false","null",
        "int","float","str","bool","list","map",
        "print","printf","debug","len","typeof",
        "math.sin","math.cos","math.tan","math.sqrt","math.abs",
        "math.floor","math.ceil","math.round","math.max","math.min",
        "math.pow","math.log","math.rand","math.rand_int",
        "math.clamp","math.lerp","math.sign",
        "math.PI","math.TAU","math.E","math.INF",
        "draw.circle","draw.rect","draw.square","draw.triangle",
        "draw.polygon","draw.diamond","draw.line","draw.parallelogram","draw.trapezoid",
        "ready","update","draw","on_exit",
        "Color.RED","Color.GREEN","Color.BLUE","Color.WHITE","Color.BLACK",
        "Color.YELLOW","Color.CYAN","Color.MAGENTA",
    ];
    if prefix.len() < 2 { return Vec::new(); }
    KEYWORDS.iter()
        .filter(|&&s| s.starts_with(prefix) && s != prefix)
        .copied().take(10).collect()
}

fn word_at(text: &str, byte_pos: usize) -> Option<(usize, String)> {
    let pos    = byte_pos.min(text.len());
    let before = &text[..pos];
    let start  = before.rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .map(|i| { let mut j = i + 1; while j < before.len() && !before.is_char_boundary(j) { j += 1; } j })
        .unwrap_or(0);
    let word   = before[start..].to_string();
    if word.is_empty() { None } else { Some((start, word)) }
}

fn char_to_byte(text: &str, char_idx: usize) -> usize {
    text.char_indices().nth(char_idx).map(|(b, _)| b).unwrap_or(text.len())
}

// ── 日本語フォント ──────────────────────────────────

fn setup_japanese_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    for path in &["C:/Windows/Fonts/meiryo.ttc","C:/Windows/Fonts/YuGothM.ttc",
                  "C:/Windows/Fonts/yugothm.ttc","C:/Windows/Fonts/msgothic.ttc"] {
        if let Ok(data) = std::fs::read(path) {
            fonts.font_data.insert("jp".to_owned(), FontData::from_owned(data));
            if let Some(v) = fonts.families.get_mut(&FontFamily::Proportional) { v.push("jp".to_owned()); }
            if let Some(v) = fonts.families.get_mut(&FontFamily::Monospace)    { v.push("jp".to_owned()); }
            break;
        }
    }
    ctx.set_fonts(fonts);
}

fn dirs_home() -> PathBuf {
    std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."))
}
