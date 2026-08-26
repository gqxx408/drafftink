#![windows_subsystem = "windows"]

mod annotation;
mod app;
mod interaction;
mod multi_page;
mod render;

/// Try to find a CJK font on the system.
/// Returns the bytes of the font file if found.
fn load_cjk_font() -> Option<(Vec<u8>, &'static str)> {
    let candidates: &[(&str, &str)] = &[
        // Windows
        ("C:\\Windows\\Fonts\\msyh.ttc", "Microsoft YaHei"),
        ("C:\\Windows\\Fonts\\msyh.ttf", "Microsoft YaHei"),
        ("C:\\Windows\\Fonts\\simhei.ttf", "SimHei"),
        ("C:\\Windows\\Fonts\\simsun.ttc", "SimSun"),
        ("C:\\Windows\\Fonts\\msyhbd.ttc", "Microsoft YaHei Bold"),
        // Linux
        (
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "Noto CJK",
        ),
        (
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            "WenQuanYi",
        ),
        ("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "DejaVu"),
        // macOS
        ("/System/Library/Fonts/PingFang.ttc", "PingFang"),
        ("/System/Library/Fonts/STHeiti Light.ttc", "STHeiti"),
    ];
    for (path, name) in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            log::info!("Loaded CJK font: {} ({} bytes)", name, bytes.len());
            return Some((bytes, name));
        }
    }
    log::warn!("No CJK font found on system; Chinese characters will show as boxes");
    None
}

/// Install CJK font into egui's font system.
fn install_cjk_fonts(ctx: &egui::Context) {
    let Some((font_data, name)) = load_cjk_font() else {
        return;
    };

    let mut fonts = egui::FontDefinitions::default();
    // Register the CJK font under a custom family name
    fonts.font_data.insert(
        "cjk".to_owned(),
        egui::FontData::from_owned(font_data),
    );
    // Make it a fallback for the default proportional family
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .push("cjk".to_owned());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .push("cjk".to_owned());
    log::info!("Installed CJK font fallback: {}", name);
    ctx.set_fonts(fonts);
}

fn main() {
    env_logger::init();
    log::info!("Drafftink Edit starting");

    let mut app = app::EditApp::default();

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

    if let Err(e) = eframe::run_native(
        "Drafftink Edit",
        native_options,
        Box::new(move |cc| {
            install_cjk_fonts(&cc.egui_ctx);
            // ── 强制深色主题：无论 Windows 系统是浅色还是深色模式，始终保持深色 UI ──
            cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
            Ok(Box::new(app) as Box<dyn eframe::App>)
        }),
    ) {
        let msg = format!("Fatal: {e}");
        log::error!("{}", msg);
        rfd::MessageDialog::new()
            .set_title("Drafftink Edit Error")
            .set_description(&msg)
            .set_level(rfd::MessageLevel::Error)
            .show();
    }
}
