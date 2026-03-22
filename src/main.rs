mod ide;
mod compiler;
mod runtime;

fn main() {
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
