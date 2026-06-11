mod ide;
mod compiler;
mod runtime;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // ── 最優先: 自己埋め込みゲームデータを検出 ──────────────────────
    // 末尾に MISTGAME マジックがあれば IDE を起動せずにそのままゲームを実行する。
    // Cargo/Rustc 不要のスタンドアロン exe として動作するコアロジック。
    if let Some(embedded) = ide::export::try_read_embedded() {
        run_embedded_game(embedded);
        return;
    }

    // ── --player <project_dir> : サブプロセスゲームプレイヤーモード ──
    if let Some(pos) = args.iter().position(|a| a == "--player") {
        let proj_dir = args.get(pos + 1)
            .map(|s| std::path::PathBuf::from(s))
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        player_main(proj_dir);
        return;
    }

    // ── 通常モード: IDE 起動 ────────────────────────────────────────
    env_logger::init();
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MistEngine IDE")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "MistEngine IDE",
        opts,
        Box::new(|cc| Ok(Box::new(ide::app::IdeApp::new(cc)))),
    ).expect("起動失敗");
}

/// 自己埋め込みゲームデータを使って直接ゲームを実行する
/// （スタンドアロン exe モード: Cargo/Rustc 不要）
fn run_embedded_game(embedded: ide::export::EmbeddedData) {
    use runtime::vm::GameState;
    use runtime::sdl_window::{GameWindowConfig, run_game_window};

    // 設定 JSON をパース
    let config: ide::project::ProjectConfig =
        match serde_json::from_str(&embedded.config_json) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[embedded] 設定パースエラー: {}", e);
                std::process::exit(1);
            }
        };

    let game_config = GameWindowConfig {
        title:      config.name.clone(),
        width:      config.window_width,
        height:     config.window_height,
        high_dpi:   config.high_dpi,
        resizable:  config.resizable,
        anti_alias: config.anti_alias,
        vsync:      config.vsync,
        // 埋め込みモードではプロジェクトディレクトリは exe と同じ場所とする
        proj_dir:   std::env::current_exe()
                        .ok()
                        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
                        .unwrap_or_else(|| std::path::PathBuf::from(".")),
    };

    let state = GameState::new();
    state.running.store(true, std::sync::atomic::Ordering::Relaxed);
    run_game_window(game_config, embedded.script, state);
}

/// ゲームプレイヤーモード（IDE からサブプロセスとして起動される）
fn player_main(proj_dir: std::path::PathBuf) {
    use runtime::vm::GameState;
    use runtime::sdl_window::{GameWindowConfig, run_game_window};

    // プロジェクト設定読み込み
    let proj = ide::project::ProjectEntry::load(&proj_dir)
        .unwrap_or_else(|| {
            eprintln!("[player] project.json が見つかりません: {:?}", proj_dir);
            std::process::exit(1);
        });

    let main_path = proj_dir.join(&proj.config.main_file);
    let script = match std::fs::read_to_string(&main_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[player] スクリプト読み込み失敗: {}", e);
            std::process::exit(1);
        }
    };

    let config = GameWindowConfig {
        title:      proj.config.name.clone(),
        width:      proj.config.window_width,
        height:     proj.config.window_height,
        high_dpi:   proj.config.high_dpi,
        resizable:  proj.config.resizable,
        anti_alias: proj.config.anti_alias,
        vsync:      proj.config.vsync,
        proj_dir:   proj_dir.clone(),
    };

    println!("[game] Game started!");
    let state = GameState::new();
    state.running.store(true, std::sync::atomic::Ordering::Relaxed);

    run_game_window(config, script, state);
}
