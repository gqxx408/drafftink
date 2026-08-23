//! Drafftink Edit — the standalone editor application.
//!
//! Usage: edit.exe [enbx_file]

mod app;
mod annotation;
mod interaction;
mod multi_page;
mod render;

use app::EditApp;

fn main() -> eframe::Result {
    env_logger::init();
    log::info!("Drafftink Edit starting");

    let mut app = EditApp::default();

    // If a file path was passed on the command line, import it
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 {
        let path = std::path::Path::new(&args[1]);
        if path.exists() {
            app.import_enbx(path);
        }
    }

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("Drafftink Edit"),
        ..Default::default()
    };

    eframe::run_native(
        "Drafftink Edit",
        native_options,
        Box::new(|_cc| Ok(Box::new(app))),
    )
}
