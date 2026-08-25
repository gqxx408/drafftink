#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;

fn main() -> eframe::Result<()> {
    eprintln!("[baseline] PID={}", std::process::id());
    eprintln!("[baseline] Starting minimal egui fullscreen window...");
    eprintln!("[baseline] Open Task Manager → Details → baseline_mem.exe → Private Memory");
    eprintln!("[baseline] Press ESC to exit.");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_fullscreen(true)
            .with_decorations(false)
            .with_title("BaselineMem"),
        ..Default::default()
    };

    eframe::run_native(
        "BaselineMem",
        options,
        Box::new(|_cc| {
            eprintln!("[baseline] eframe initialized");
            Ok(Box::new(BaselineApp))
        }),
    )
}

struct BaselineApp;

impl eframe::App for BaselineApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            eprintln!("[baseline] ESC pressed, exiting...");
            std::process::exit(0);
        }
    }
}
