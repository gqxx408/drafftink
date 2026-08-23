//! SeewoClass MVP — entry point.
//!
//! Launches the egui/eframe-based courseware editor window.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

mod annotation;
mod animation_player;
mod app;
mod interaction;
mod io;
mod logger;
mod multi_page;
mod render;

use app::SeewoClassApp;

fn main() -> Result<(), eframe::Error> {
    // enbx_importer always debug; rest defaults to info (debug if RUST_LOG=debug).
    let level = if std::env::var("RUST_LOG").is_ok() {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    let _guard = logger::init(level);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("SeewoClass MVP — Courseware Editor"),
        ..Default::default()
    };

    eframe::run_native(
        "SeewoClass MVP",
        options,
        Box::new(|cc| {
            // Load CJK fonts for Chinese text support
            load_cjk_fonts(&cc.egui_ctx);
            // ── 强制深色主题：无论 Windows 系统是浅色还是深色模式，始终保持深色 UI ──
            cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
            Ok(Box::new(SeewoClassApp::default()))
        }),
    )
}

/// Attempt to load system CJK fonts so that Chinese text renders correctly.
fn load_cjk_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // System font paths for different platforms
    let cjk_paths: &[&str] = if cfg!(target_os = "windows") {
        &[
            "C:\\Windows\\Fonts\\msyh.ttc",
            "C:\\Windows\\Fonts\\simhei.ttf",
            "C:\\Windows\\Fonts\\simsun.ttc",
        ]
    } else if cfg!(target_os = "macos") {
        &[
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/STHeiti Light.ttc",
        ]
    } else {
        &[
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        ]
    };

    for (i, path) in cjk_paths.iter().enumerate() {
        let name = format!("cjk_{i}");
        if fonts.font_data.contains_key(&name) {
            continue;
        }
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(
                name.clone(),
                egui::FontData::from_owned(bytes).tweak(egui::FontTweak {
                    scale: 1.0,
                    ..Default::default()
                }),
            );
        }
    }

    // Prepend CJK fonts to the proportional family (last loaded = highest priority)
    for i in (0..cjk_paths.len()).rev() {
        let name = format!("cjk_{i}");
        if fonts.font_data.contains_key(&name) {
            fonts
                .families
                .get_mut(&egui::FontFamily::Proportional)
                .unwrap()
                .insert(0, name);
        }
    }

    ctx.set_fonts(fonts);
}
