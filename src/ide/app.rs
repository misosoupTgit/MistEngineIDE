/// MistEngine IDE – メインアプリ (eframe::App)

use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;
use egui::*;

use crate::ide::{
    theme::{Theme, ThemeKind},
    editor::{EditorState, build_layout_job},
    explorer::{ExplorerState, FileNode},
    console::{ConsoleState, ConsoleLine, LineKind},
    project::{ProjectEntry, NewProjectParams, create_project, scan_projects},
};
use crate::compiler::{self, CompileResult, cache::CompileCache};
use crate::runtime::vm::{GameState, DrawCmd, run_game};
use crate::runtime::sdl_window::{GameWindowConfig, run_sdl2_window};

enum BuildMsg {
    Log(String),
    Warn(String),
    Error(String),
    Done { exe: PathBuf },
    Failed,
}

#[derive(PartialEq)]
enum Screen { ThemeSelect, ProjectSelect, Ide }

pub struct IdeApp {
    screen:          Screen,
    theme:           Theme,
    editor:          EditorState,
    explorer:        ExplorerState,
    console:         ConsoleState,
    project:         Option<ProjectEntry>,
    projects:        Vec<ProjectEntry>,
    cache:           CompileCache,
    new_proj_open:   bool,
    new_proj_params: NewProjectParams,
    new_proj_err:    String,
    // コード補完
    ac_suggestions:  Vec<&'static str>,
    ac_sel:          usize,
    ac_word_start:   usize,
    ac_insert:       Option<String>,
    // バックグラウンドビルド（下位互換用）
    build_rx:        Option<mpsc::Receiver<BuildMsg>>,
    build_env:       Option<(String, u32, u32)>,
    // インタープリターゲーム
    game_state:  Option<GameState>,
    game_thread: Option<std::thread::JoinHandle<()>>,
    sdl_thread:  Option<std::thread::JoinHandle<()>>,
}

impl IdeApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_japanese_fonts(&cc.egui_ctx);
        let theme = Theme::dark();
        theme.apply(&cc.egui_ctx);
        let mut style = (*cc.egui_ctx.style()).clone();
        style.text_styles.insert(TextStyle::Body,      FontId::new(14.0, FontFamily::Proportional));
        style.text_styles.insert(TextStyle::Monospace, FontId::new(14.0, FontFamily::Monospace));
        style.text_styles.insert(TextStyle::Small,     FontId::new(12.0, FontFamily::Proportional));
        cc.egui_ctx.set_style(style);
        IdeApp {
            screen:          Screen::ThemeSelect,
            theme,
            editor:          EditorState::new(),
            explorer:        ExplorerState::new(),
            console:         ConsoleState::new(),
            project:         None,
            projects:        Vec::new(),
            cache:           CompileCache::new(&PathBuf::from(".")),
            new_proj_open:   false,
            new_proj_params: NewProjectParams::default(),
            new_proj_err:    String::new(),
            ac_suggestions:  Vec::new(),
            ac_sel:          0,
            ac_word_start:   0,
            ac_insert:       None,
            build_rx:        None,
            build_env:       None,
            game_state:      None,
            game_thread:     None,
            sdl_thread:      None,
        }
    }

    fn set_theme(&mut self, kind: ThemeKind, ctx: &egui::Context) {
        self.theme = Theme::from_kind(kind);
        self.theme.apply(ctx);
    }

    fn open_project(&mut self, proj: ProjectEntry) {
        let main_path = proj.path.join(&proj.config.main_file);
        self.explorer.set_root(&proj.path);
        self.explorer.selected = Some(main_path.clone());
        if let Err(e) = self.editor.load_file(&main_path) {
            self.console.push(ConsoleLine::error(format!("読み込み失敗: {}", e)));
        } else {
            self.console.push(ConsoleLine::normal(format!("Project '{}' を開きました。", proj.config.name)));
            self.console.push(ConsoleLine::normal("Ctrl+S=保存  Ctrl+R=コンパイル  Tab=補完確定"));
        }
        self.cache = CompileCache::new(&proj.path);
        self.project = Some(proj);
        self.screen = Screen::Ide;
    }

    fn run(&mut self) {
        // すでに実行中なら停止
        if let Some(gs) = &self.game_state {
            gs.running.store(false, std::sync::atomic::Ordering::Relaxed);
            self.game_state  = None;
            self.game_thread = None;
            self.sdl_thread  = None; // SDL2スレッドも running=false で自動終了
            self.console.push(ConsoleLine::warn("⏹ ゲームを停止しました"));
            return;
        }

        self.console.push(ConsoleLine::normal("▶ パース中..."));
        let src  = self.editor.text.clone();
        let path = self.editor.file_path.clone().unwrap_or_else(|| PathBuf::from("main.mist"));

        // ── Mistral → AST パース ──
        let stmts = match crate::compiler::parse_only(&src, &path) {
            Ok(stmts) => stmts,
            Err(errs) => {
                self.console.push(ConsoleLine::error(format!("パースエラー {} 件:", errs.len())));
                self.console.push_compile_errors(&errs);
                return;
            }
        };

        // ── プロジェクト設定を毎回ディスクから最新取得 ──
        // 優先度: キャッシュ済みパスのJSON再読み → ファイルパス親dir → デフォルト
        let proj_config = if let Some(proj) = &self.project {
            crate::ide::project::ProjectEntry::load(&proj.path)
                .map(|pe| pe.config)
                .unwrap_or_else(|| proj.config.clone())
        } else if let Some(fp) = &self.editor.file_path {
            fp.parent()
                .and_then(|dir| crate::ide::project::ProjectEntry::load(dir))
                .map(|pe| pe.config)
                .unwrap_or_default()
        } else {
            crate::ide::project::ProjectConfig::default()
        };

        let proj_name  = proj_config.name.clone();
        let win_w      = proj_config.window_width;
        let win_h      = proj_config.window_height;
        let high_dpi   = proj_config.high_dpi;
        let resizable  = proj_config.resizable;
        let anti_alias = proj_config.anti_alias;
        let vsync      = proj_config.vsync;

        self.console.push(ConsoleLine::normal(format!(
            "📐 ウィンドウ {}x{} (aa={}, vsync={}, resizable={})", win_w, win_h, anti_alias, vsync, resizable
        )));

        // ── ゲーム状態を初期化 ──
        let gs     = GameState::new();
        let gs_vm  = gs.clone_arcs();
        let gs_sdl = gs.clone_arcs();

        // ── VMスレッド ──
        let vm_handle = std::thread::spawn(move || {
            run_game(stmts, gs_vm, 60);
        });

        // ── minifbスレッド（独立ウィンドウ・独立60fps）──
        let sdl_title  = proj_name.clone();
        let sdl_handle = std::thread::spawn(move || {
            run_sdl2_window(
                GameWindowConfig { title: sdl_title, width: win_w, height: win_h,
                                   high_dpi, resizable, anti_alias, vsync },
                gs_sdl.draw_cmds,
                gs_sdl.bg_color,
                gs_sdl.held_keys,
                gs_sdl.running,
            );
        });

        self.game_state  = Some(gs);
        self.game_thread = Some(vm_handle);
        self.sdl_thread  = Some(sdl_handle);
        self.console.push(ConsoleLine::normal(
            format!("🎮 {} 実行中 ({}×{}) — 再度 Run で停止", proj_name, win_w, win_h)
        ));
    }


    fn build(&mut self) { self.console.push(ConsoleLine::normal("⬛ ビルド (WIP)")); }
    fn save(&mut self) {
        match self.editor.save_file() {
            Ok(_)  => self.console.push(ConsoleLine::normal("✓ 保存しました")),
            Err(e) => self.console.push(ConsoleLine::error(format!("保存失敗: {}", e))),
        }
    }

    // ── テーマ選択 ──────────────────────────────────────

    fn ui_theme_select(&mut self, ctx: &egui::Context) {
        CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(70.0);
                ui.label(RichText::new("MistEngine IDE").size(30.0).color(self.theme.text_accent).strong());
                ui.add_space(6.0);
                ui.label(RichText::new("テーマを選択してください").color(self.theme.text_muted).size(14.0));
                ui.add_space(36.0);
                ui.horizontal(|ui| {
                    let card_w = 150.0;
                    let gap    = 16.0;
                    let total  = card_w * 4.0 + gap * 3.0;
                    let margin = ((ui.available_width() - total) / 2.0).max(0.0);
                    ui.add_space(margin);
                    for &kind in ThemeKind::all() {
                        let pt = Theme::from_kind(kind);
                        let (rect, resp) = ui.allocate_exact_size(vec2(card_w, 140.0), Sense::click());
                        let painter = ui.painter();
                        painter.rect_filled(rect, 10.0, pt.panel_bg);
                        painter.rect_stroke(rect, 10.0, Stroke::new(1.5,
                            if resp.hovered() { pt.accent } else { pt.border }));
                        let preview = Rect::from_min_size(rect.min + vec2(10.0, 10.0), vec2(card_w - 20.0, 80.0));
                        painter.rect_filled(preview, 6.0, pt.editor_bg);
                        let cols = [pt.syn_keyword, pt.syn_func, pt.syn_string, pt.syn_type, pt.syn_number, pt.syn_comment];
                        for (i, &c) in cols.iter().enumerate() {
                            let dot = Rect::from_min_size(
                                preview.min + vec2(8.0 + (i%3) as f32 * 38.0, 16.0 + (i/3) as f32 * 24.0),
                                vec2(30.0, 10.0));
                            painter.rect_filled(dot, 3.0, c);
                        }
                        painter.text(rect.center_bottom() - vec2(0.0, 18.0),
                            Align2::CENTER_CENTER, kind.name(), FontId::proportional(13.0), pt.text);
                        if resp.clicked() {
                            self.set_theme(kind, ctx);
                            self.screen = Screen::ProjectSelect;
                            self.projects = scan_projects(&dirs_home());
                        }
                        ui.add_space(gap);
                    }
                });
            });
        });
    }

    // ── プロジェクト選択 ─────────────────────────────────

    fn ui_project_select(&mut self, ctx: &egui::Context) {
        let mut close_dialog  = false;
        let mut create_result: Option<Result<ProjectEntry, std::io::Error>> = None;
        if self.new_proj_open {
            egui::Window::new("新規プロジェクト")
                .collapsible(false).resizable(false)
                .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .fixed_size(vec2(380.0, 240.0))
                .show(ctx, |ui| {
                    ui.add_space(8.0);
                    ui.label(RichText::new("プロジェクト名").color(self.theme.text_muted).small());
                    ui.add(TextEdit::singleline(&mut self.new_proj_params.name).min_size(vec2(340.0, 28.0)));
                    ui.add_space(8.0);
                    ui.label(RichText::new("ウィンドウサイズ").color(self.theme.text_muted).small());
                    ui.horizontal(|ui| {
                        ui.label("W:");
                        ui.add(DragValue::new(&mut self.new_proj_params.window_width).speed(1.0).range(320u32..=3840u32));
                        ui.label(" H:");
                        ui.add(DragValue::new(&mut self.new_proj_params.window_height).speed(1.0).range(240u32..=2160u32));
                    });
                    if !self.new_proj_err.is_empty() {
                        ui.colored_label(self.theme.con_error, &self.new_proj_err);
                    }
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button("キャンセル").clicked() { close_dialog = true; }
                        ui.add_space(8.0);
                        if ui.add(Button::new("  作成  ").fill(self.theme.accent)).clicked() {
                            create_result = Some(create_project(&self.new_proj_params));
                        }
                    });
                });
            if close_dialog { self.new_proj_open = false; }
            if let Some(r) = create_result {
                match r {
                    Ok(proj) => { self.new_proj_open = false; self.open_project(proj); return; }
                    Err(e)   => { self.new_proj_err = format!("作成失敗: {}", e); }
                }
            }
        }
        CentralPanel::default().show(ctx, |ui| {
            ui.add_space(40.0);
            ui.horizontal(|ui| {
                ui.add_space(40.0);
                ui.heading(RichText::new("プロジェクトを選択").color(self.theme.text).size(20.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(40.0);
                    if ui.add(Button::new("  ＋ 新規プロジェクト  ").fill(self.theme.accent)).clicked() {
                        self.new_proj_open = true; self.new_proj_err.clear();
                    }
                });
            });
            ui.add_space(12.0);
            let projects = self.projects.clone();
            ScrollArea::vertical().show(ui, |ui| {
                for proj in &projects {
                    let pc = proj.clone();
                    let resp = ui.group(|ui| {
                        ui.set_width(ui.available_width() - 80.0);
                        ui.horizontal(|ui| {
                            ui.add_space(4.0);
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&proj.config.name).color(self.theme.text).strong().size(14.0));
                                ui.label(RichText::new(proj.path.to_string_lossy().as_ref()).color(self.theme.text_muted).small());
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(RichText::new(format!("{}×{}", proj.config.window_width, proj.config.window_height)).color(self.theme.text_muted).small());
                            });
                        });
                    }).response;
                    if resp.interact(Sense::click()).clicked() { self.open_project(pc); return; }
                    ui.add_space(4.0);
                }
                if projects.is_empty() {
                    ui.add_space(40.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("プロジェクトが見つかりません。新規作成してください。").color(self.theme.text_muted));
                    });
                }
            });
        });
    }

    // ── IDE メイン ────────────────────────────────────────

    fn ui_ide(&mut self, ctx: &egui::Context) {
        // ── ゲーム実行中: コンソール出力ポーリング + 停止検知 ──
        if self.game_state.is_some() {
            // SDL2ウィンドウが独立して60fps描画するため、IDEは100msごとのポーリングで十分
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            let mut game_stopped = false;

            if let Some(gs) = &self.game_state {
                // print() の出力を IDE コンソールに転送
                if let Ok(mut q) = gs.console.try_lock() {
                    for line in q.drain(..) {
                        self.console.push(ConsoleLine::normal(format!("[game] {}", line)));
                    }
                }
                // VM スレッドが終了していたら掃除
                if !gs.running.load(std::sync::atomic::Ordering::Relaxed) {
                    game_stopped = true;
                }
            }

            if game_stopped {
                self.game_state  = None;
                self.game_thread = None;
                self.sdl_thread  = None;
                self.console.push(ConsoleLine::normal("⏹ ゲーム終了"));
            }
        }

        // ※ show_viewport_deferred は SDL2 スレッドに移行済みのため不要

        if self.build_rx.is_some() {
            ctx.request_repaint();
            let mut finished = false;
            if let Some(rx) = &self.build_rx {
                loop {
                    match rx.try_recv() {
                        Ok(BuildMsg::Log(s))   => self.console.push(ConsoleLine::normal(s)),
                        Ok(BuildMsg::Warn(s))  => self.console.push(ConsoleLine::warn(s)),
                        Ok(BuildMsg::Error(s)) => self.console.push(ConsoleLine::error(s)),
                        Ok(BuildMsg::Done{..}) | Ok(BuildMsg::Failed) => { finished = true; break; }
                        Err(std::sync::mpsc::TryRecvError::Empty)        => break,
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => { finished = true; break; }
                    }
                }
            }
            if finished { self.build_rx = None; self.build_env = None; }
        }


        // 補完確定（前フレームで生成された挿入テキストをここで適用）
        if let Some(ins) = self.ac_insert.take() {
            // ac_word_start から ac_word_start + 現在入力中の単語長 を置換
            // カーソル位置はeguiのTextEditOutputから取得できないため、
            // word_at()で再計算した単語末尾(= ac_word_start + word.len())を使う
            let text_snap = self.editor.text.clone();
            let cursor_byte = if let Some((_ws, word)) = word_at(&text_snap, self.ac_word_start + ins.len().max(1)) {
                // word_start + 入力単語の長さを単語末尾とする
                self.ac_word_start + word.len()
            } else {
                // フォールバック: ac_word_startから前方一致する単語末尾を探す
                let after = &text_snap[self.ac_word_start..];
                let word_len = after.find(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
                    .unwrap_or(after.len());
                self.ac_word_start + word_len
            };

            if self.ac_word_start <= cursor_byte && cursor_byte <= self.editor.text.len() {
                self.editor.text.replace_range(self.ac_word_start..cursor_byte, &ins);
                self.editor.dirty = true;
            }
            self.ac_suggestions.clear();
        }

        // キーバインドと補完操作を1回の input() 呼び出しで処理（イベント重複消費を防ぐ）
        let (do_save, do_run, do_build, ac_confirmed, ac_escape) = ctx.input(|i| {
            let save    = i.key_pressed(Key::S) && i.modifiers.ctrl;
            let run     = i.key_pressed(Key::R) && i.modifiers.ctrl;
            let build   = i.key_pressed(Key::B) && i.modifiers.ctrl;
            let confirm = i.key_pressed(Key::Enter) || i.key_pressed(Key::Tab);
            let escape  = i.key_pressed(Key::Escape);
            (save, run, build, confirm, escape)
        });
        if do_save  { self.save(); }
        if do_run   { self.run(); }
        if do_build { self.build(); }

        // 補完候補操作（補完がアクティブなときのみ Tab/Enter を消費）
        if !self.ac_suggestions.is_empty() {
            if ac_confirmed {
                let chosen = self.ac_suggestions[self.ac_sel].to_string();
                self.ac_insert = Some(chosen);
            }
            if ac_escape {
                self.ac_suggestions.clear();
            }
        }

        // ツールバー色をコピー（borrow回避）
        let dirty     = self.editor.dirty;
        let is_running = self.game_state.is_some();
        let proj_nm  = self.project.as_ref().map(|p| p.config.name.clone()).unwrap_or_else(|| "MistEngine".to_string());
        let title    = format!("{}{}{}", proj_nm, if dirty { " ●" } else { "" }, if is_running { " [実行中]" } else { "" });
        let c_accent = self.theme.text_accent;
        let c_muted  = self.theme.text_muted;
        let c_btn    = self.theme.button_bg;
        let c_build  = self.theme.build_btn;
        let c_run    = if is_running { egui::Color32::from_rgb(200, 80, 60) } else { self.theme.run_btn };
        let c_con_n  = self.theme.con_normal;
        let c_con_d  = self.theme.con_debug;
        let c_con_w  = self.theme.con_warn;
        let c_con_e  = self.theme.con_error;
        let console_lines = self.console.lines.clone();
        let mut do_clear  = false;

        // ─ ツールバー ─
        TopBottomPanel::top("toolbar").exact_height(40.0).show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.add_space(8.0);
                ui.label(RichText::new("MistEngine").color(c_accent).strong());
                ui.separator();
                ui.label(RichText::new(&title).color(c_muted).small());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(8.0);
                    if ui.add(Button::new("保存").fill(c_btn)).clicked()      { self.save(); }
                    ui.add_space(4.0);
                    if ui.add(Button::new("⬛ Build").fill(c_build)).clicked() { self.build(); }
                    ui.add_space(4.0);
                    let run_label = if is_running { "⏹ Stop" } else { "▶ Run" };
                    if ui.add(Button::new(run_label).fill(c_run)).clicked() { self.run(); }
                });
            });
        });

        // ─ コンソール ─
        // resizable が空のときでも機能するよう、パネルの高さを永続メモリに保存
        TopBottomPanel::bottom("console")
            .min_height(80.0)
            .max_height(400.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Console").small().strong().color(c_muted));
                    ui.separator();
                    ui.label(RichText::new(format!("{} 行", console_lines.len())).color(c_muted).small());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("✕ クリア").clicked() { do_clear = true; }
                    });
                });
                ui.separator();
                // ★ min_scrolled_height: コンテンツが空でもScrollAreaが最低高さを持つ
                //    → TopBottomPanel のリサイズハンドルが常に機能する
                ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .min_scrolled_height(50.0) // 空でも最低50pxのスクロール領域を確保
                    .auto_shrink([false, false]) // 縮まない（パネルの高さを維持）
                    .show(ui, |ui| {
                        if console_lines.is_empty() {
                            // 空のときもスペースを確保してリサイズを可能にする
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new("コンソール出力はありません")
                                    .color(Color32::from_gray(80))
                                    .italics()
                                    .size(12.0)
                            );
                        } else {
                            for line in &console_lines {
                                let c = match line.kind {
                                    LineKind::Normal => c_con_n,
                                    LineKind::Debug  => c_con_d,
                                    LineKind::Warn   => c_con_w,
                                    LineKind::Error  => c_con_e,
                                };
                                ui.label(RichText::new(&line.text).color(c).monospace().size(13.0));
                            }
                        }
                    });
            });

        if do_clear { self.console.clear(); }

        // ─ エクスプローラー ─
        SidePanel::left("explorer").width_range(140.0..=360.0).resizable(true).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("EXPLORER").small().strong().color(c_muted));
            });
            ui.separator();
            ScrollArea::vertical().show(ui, |ui| {
                let mut open_req: Option<PathBuf> = None;
                if let Some(root) = &mut self.explorer.root {
                    open_req = render_tree(ui, root, &self.explorer.selected, &self.theme);
                }
                if let Some(path) = open_req {
                    self.explorer.selected = Some(path.clone());
                    match self.editor.load_file(&path) {
                        Ok(_) => self.console.push(ConsoleLine::normal(
                            format!("開きました: {}", path.file_name().unwrap_or_default().to_string_lossy())
                        )),
                        Err(e) => self.console.push(ConsoleLine::error(format!("エラー: {}", e))),
                    }
                }
            });
        });

        // ─ エディタ ─
        let gutter_bg   = self.theme.gutter_bg;
        let editor_bg   = self.theme.editor_bg;
        let ln_color    = self.theme.line_number;
        let ac_bg       = self.theme.panel_bg;
        let ac_accent   = self.theme.accent;
        let ac_text     = self.theme.text;
        let ac_muted    = self.theme.text_muted;
        let theme_clone = self.theme.clone();
        let raw_lines   = self.editor.text.matches('\n').count();
        let line_count  = (raw_lines + 1).max(1);

        // 補完候補スナップショット（borrowを回避するため）
        let ac_suggestions = self.ac_suggestions.clone();
        let ac_sel         = self.ac_sel;

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
                // ★ フォントは TextEdit と同じ 14px を使うこと。
                //   サイズが違うと ascent（文字上端〜ベースライン）が異なり
                //   CENTER 揃えでも視覚的にずれて見える。
                //   Align2::RIGHT_TOP + row.rect.min.y で行の上端に揃えれば
                //   TextEdit の文字列とベースラインが完全一致する。
                let galley_pos   = te.galley_pos;
                let clip         = ui.painter().with_clip_rect(gutter_rect);
                let mut line_num = 1usize;
                for row in &te.galley.rows {
                    let row_top_y = galley_pos.y + row.rect.min.y; // 行の上端（スクリーン座標）
                    if row_top_y < gutter_rect.top() - 40.0 {
                        if row.ends_with_newline { line_num += 1; }
                        continue;
                    }
                    if row_top_y > gutter_rect.bottom() + 40.0 { break; }
                    clip.text(
                        pos2(gutter_rect.right() - 6.0, row_top_y),
                        Align2::RIGHT_TOP,   // ← 行の上端を揃える
                        format!("{}", line_num),
                        FontId::monospace(14.0), // ← TextEdit と同じフォントサイズ
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

// ── ゲーム描画コマンドを egui Painter で実行 ──────────────────
fn render_draw_cmd(painter: &egui::Painter, cmd: &DrawCmd, origin: egui::Pos2) {
    use egui::{Color32, Pos2, Vec2, Rect, Stroke};

    let c4 = |col: &[f32; 4]| -> Color32 {
        Color32::from_rgba_unmultiplied(
            (col[0].clamp(0.,1.)*255.) as u8,
            (col[1].clamp(0.,1.)*255.) as u8,
            (col[2].clamp(0.,1.)*255.) as u8,
            (col[3].clamp(0.,1.)*255.) as u8,
        )
    };

    match cmd {
        DrawCmd::Circle { x, y, r, color } => {
            painter.circle_filled(
                Pos2::new(origin.x + x, origin.y + y), *r, c4(color));
        }
        DrawCmd::Rect { x, y, w, h, color } => {
            painter.rect_filled(
                Rect::from_min_size(Pos2::new(origin.x + x, origin.y + y), Vec2::new(*w, *h)),
                0.0, c4(color));
        }
        DrawCmd::Square { x, y, s, color } => {
            painter.rect_filled(
                Rect::from_min_size(Pos2::new(origin.x + x, origin.y + y), Vec2::splat(*s)),
                0.0, c4(color));
        }
        DrawCmd::Line { x1, y1, x2, y2, color } => {
            painter.line_segment(
                [Pos2::new(origin.x + x1, origin.y + y1),
                 Pos2::new(origin.x + x2, origin.y + y2)],
                Stroke::new(2.0, c4(color)));
        }
        DrawCmd::Triangle { x, y, s, color } => {
            let (cx, cy, h) = (origin.x + x, origin.y + y, s * 0.866);
            painter.add(egui::Shape::convex_polygon(vec![
                Pos2::new(cx,           cy - h * 0.667),
                Pos2::new(cx + s * 0.5, cy + h * 0.333),
                Pos2::new(cx - s * 0.5, cy + h * 0.333),
            ], c4(color), Stroke::NONE));
        }
        DrawCmd::Polygon { x, y, s, sides, color } => {
            let n = (*sides).max(3) as usize;
            let (cx, cy) = (origin.x + x, origin.y + y);
            let pts: Vec<Pos2> = (0..n).map(|i| {
                let a = std::f32::consts::TAU * i as f32 / n as f32
                        - std::f32::consts::FRAC_PI_2;
                Pos2::new(cx + s * a.cos(), cy + s * a.sin())
            }).collect();
            painter.add(egui::Shape::convex_polygon(pts, c4(color), Stroke::NONE));
        }
        DrawCmd::Diamond { x, y, s, color } => {
            let (cx, cy) = (origin.x + x, origin.y + y);
            painter.add(egui::Shape::convex_polygon(vec![
                Pos2::new(cx,    cy - s),
                Pos2::new(cx + s, cy),
                Pos2::new(cx,    cy + s),
                Pos2::new(cx - s, cy),
            ], c4(color), Stroke::NONE));
        }
        DrawCmd::Text { x, y, text, size, color } => {
            painter.text(
                Pos2::new(origin.x + x, origin.y + y),
                egui::Align2::LEFT_TOP,
                text,
                egui::FontId::proportional(*size),
                c4(color),
            );
        }
        DrawCmd::Background(_) => { /* 背景はCentralPanelのfill色で対応済み */ }
    }
}
