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
    use runtime::vm::{GameState, Interpreter};
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

    // スクリプトをパース
    let dummy_path = std::path::PathBuf::from("embedded.mist");
    let stmts = match compiler::parse_only(&embedded.script, &dummy_path) {
        Ok(s) => s,
        Err(errs) => {
            for e in &errs { eprintln!("[embedded] パースエラー: {:?}", e); }
            std::process::exit(1);
        }
    };

    // GameState と Interpreter を初期化
    let state = GameState::new();
    let mut interp = Interpreter::new(state.clone_arcs());

    if let Err(e) = interp.exec_stmts(&stmts).map(|_| ()) {
        eprintln!("[embedded] トップレベルエラー: {}", e);
        std::process::exit(1);
    }

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

    state.running.store(true, std::sync::atomic::Ordering::Relaxed);
    run_game_window(game_config, interp, state);
}

/// ゲームプレイヤーモード（IDE からサブプロセスとして起動される）
fn player_main(proj_dir: std::path::PathBuf) {
    use runtime::vm::{GameState, Interpreter};
    use runtime::sdl_window::{GameWindowConfig, run_game_window};

    // プロジェクト設定読み込み
    let proj = ide::project::ProjectEntry::load(&proj_dir)
        .unwrap_or_else(|| {
            eprintln!("[player] project.mist.json が見つかりません: {:?}", proj_dir);
            std::process::exit(1);
        });

    let main_path = proj_dir.join(&proj.config.main_file);
    let src = match std::fs::read_to_string(&main_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[player] ソース読み込み失敗: {}", e);
            std::process::exit(1);
        }
    };

    let stmts = match compiler::parse_only(&src, &main_path) {
        Ok(s) => s,
        Err(errs) => {
            for e in &errs { eprintln!("[player] パースエラー: {:?}", e); }
            std::process::exit(1);
        }
    };

    let state  = GameState::new();
    let mut interp = Interpreter::new(state.clone_arcs());

    if let Err(e) = interp.exec_stmts(&stmts).map(|_| ()) {
        eprintln!("[player] トップレベルエラー: {}", e);
        std::process::exit(1);
    }

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
    state.running.store(true, std::sync::atomic::Ordering::Relaxed);

    run_game_window(config, interp, state);
}
