//! enbx_viewer — standalone binary for testing .enbx rendering.
//!
//! Usage:  enbx_viewer <path-to-enbx-file>

use std::path::PathBuf;

use eframe::egui;
use egui::{Align2, Color32, FontId, Pos2};

use drafftink_core::model::{CoursewareDoc, Element};
use drafftink_core::plugin::api::DummyContext;

/// Wrapper so the generic lifetime is satisfied.
struct Context(DummyContext);
impl drafftink_core::plugin::api::PluginContext for Context {
    fn log(&self, level: &str, msg: &str) {
        self.0.log(level, msg);
    }
    fn read_file(&self, path: &str) -> Result<Vec<u8>, String> {
        self.0.read_file(path)
    }
    fn system_info(&self) -> Vec<(String, String)> {
        self.0.system_info()
    }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: enbx_viewer <path-to-enbx-file>");
        std::process::exit(1);
    }
    let path = PathBuf::from(&args[1]);
    if !path.exists() {
        eprintln!("File not found: {:?}", path);
        std::process::exit(1);
    }

    // Load .enbx
    let data = std::fs::read(&path).expect("read file");
    let ctx = Context(DummyContext);
    let doc = format_enbx::loader::load_enbx(&data, &ctx).expect("load enbx");

    eprintln!("Loaded {} page(s)", doc.pages.len());
    for (i, p) in doc.pages.iter().enumerate() {
        eprintln!("  Page {}: {} elements", i, p.elements.len());
    }

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title(format!(
            "enbx_viewer — {}",
            path.file_name().unwrap().to_string_lossy()
        )),
        ..Default::default()
    };

    eframe::run_native(
        "enbx_viewer",
        native_options,
        Box::new(move |_cc| Ok(Box::new(App { doc, page: 0 }))),
    )
}

struct App {
    doc: CoursewareDoc,
    page: usize,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Navigation
        egui::TopBottomPanel::top("nav").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("◀ Prev").clicked() && self.page > 0 {
                    self.page -= 1;
                }
                ui.label(format!(
                    "Page {}/{}",
                    self.page + 1,
                    self.doc.pages.len().max(1)
                ));
                if ui.button("Next ▶").clicked() && self.page + 1 < self.doc.pages.len() {
                    self.page += 1;
                }
                ui.separator();
                ui.label(format!(
                    "Canvas: {}x{}",
                    self.doc.page_size[0], self.doc.page_size[1]
                ));
            });
        });

        // Canvas
        egui::CentralPanel::default().show(ctx, |ui| {
            let size = ctx.available_rect();
            let painter = ui.painter();

            // White background
            painter.rect_filled(size, 0.0, Color32::WHITE);

            // Render elements
            if let Some(page) = self.doc.pages.get(self.page) {
                for element in &page.elements {
                    if let Element::Text(t) = element {
                        let x = t.base.position[0];
                        let y = t.base.position[1];
                        let c = t.base.fill_color;

                        painter.text(
                            Pos2::new(x, y),
                            Align2::LEFT_TOP,
                            &t.text,
                            FontId::proportional(t.font_size),
                            c,
                        );
                    } // shapes/images not supported yet
                }
            }
        });

        // ESC to quit
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            std::process::exit(0);
        }
    }
}
