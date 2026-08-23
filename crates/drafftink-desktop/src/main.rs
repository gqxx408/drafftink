//! # drafftink-desktop
//!
//! Teacher's desktop application for lesson preparation, teaching, and grading.
//!
//! Built with egui-native (eframe) using the wgpu renderer.  Reuses
//! `drafftink-core` logic and integrates the `drafftink-enbx` compatibility
//! module for importing/exporting Seewo `.enbx` courseware files.
//!
//! ## Features
//!
//! - **备课 (Prepare)** — lesson preparation with canvas, element toolbar,
//!   slide list, and `.drftx` / `.enbx` import/export.
//! - **上课 (Teach)** — presentation mode with blackboard sync, quick quiz,
//!   real-time statistics, and annotation tools.
//! - **批改 (Grade)** — student submission list, drftx viewer, grading panel
//!   with score / comment / voice annotation, and red-pen overlay.
//! - **设置 (Settings)** — backend URL, login, plugin management, theme.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod stroke_conv;
mod video_player;
mod interactive_rect;
mod shape_renderer;
mod save;
mod audio_player;
mod undo;
mod tools;
mod function_parser;

use app::IntegratedApp;

fn main() -> Result<(), eframe::Error> {
    // ── Logging ──
    let level = if std::env::var("RUST_LOG").is_ok() {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    let _guard = init_logger(level);

    log::info!("=== drafftink-desktop starting ===");

    // ── eframe options with wgpu renderer ──
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([1024.0, 680.0])
            .with_title("Drafftink Desktop — 备课 · 上课 · 批改"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "drafftink-desktop",
        options,
        Box::new(|cc| {
            // Load CJK fonts for Chinese text rendering.
            install_cjk_fonts(&cc.egui_ctx);

            // ── Force dark theme ──
            cc.egui_ctx.set_theme(egui::ThemePreference::Dark);

            Ok(Box::new(IntegratedApp::new()))
        }),
    )
}

// ════════════════════════════════════════════════════════════════════════════
//  Logger
// ════════════════════════════════════════════════════════════════════════════

/// Minimal logger that prints to stderr.
///
/// We avoid pulling in `env_logger` or `simplelog` to keep the dependency
/// tree lean.  The guard keeps the logger alive for the entire process.
struct LoggerGuard;

impl Drop for LoggerGuard {
    fn drop(&mut self) {
        log::set_max_level(log::LevelFilter::Off);
    }
}

fn init_logger(level: log::LevelFilter) -> LoggerGuard {
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(level);
    LoggerGuard
}

static LOGGER: SimpleLogger = SimpleLogger;

struct SimpleLogger;

impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!(
                "[{} {:5}] {}",
                record.target(),
                record.level(),
                record.args(),
            );
        }
    }

    fn flush(&self) {}
}

// ════════════════════════════════════════════════════════════════════════════
//  CJK Font Loading
// ════════════════════════════════════════════════════════════════════════════

/// Attempt to load system CJK fonts so that Chinese text renders correctly.
fn install_cjk_fonts(ctx: &egui::Context) {
    let candidates: &[&str] = if cfg!(target_os = "windows") {
        &[
            "C:\\Windows\\Fonts\\msyh.ttc",
            "C:\\Windows\\Fonts\\msyh.ttf",
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
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        ]
    };

    let mut fonts = egui::FontDefinitions::default();

    // 只装载**第一个**可用的 CJK 字体。
    //
    // 早期实现会把候选表里所有存在的字体全部读进内存（msyh.ttc ≈ 19 MB、
    // simhei ≈ 10 MB、simsun.ttc ≈ 16 MB），并且每一款都同时挂进 Proportional 与
    // Monospace 两个家族。后备字体只有在前一款缺字时才会被查询，中文场景下
    // 首选字体已完全覆盖，多出来的几款纯属常驻内存浪费，还会让 egui 为每个家族
    // 维护更长的字体链、字形图集也随之膨胀。
    const CJK_FONT: &str = "cjk";
    let mut loaded_from: Option<&str> = None;

    for path in candidates {
        match std::fs::read(path) {
            Ok(bytes) => {
                log::info!("[font] CJK font loaded: {path} ({} KB)", bytes.len() / 1024);
                fonts
                    .font_data
                    .insert(CJK_FONT.to_owned(), egui::FontData::from_owned(bytes));
                loaded_from = Some(path);
                break;
            }
            Err(_) => continue,
        }
    }

    if loaded_from.is_some() {
        // 置于比例字体家族首位，取得最高优先级。
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, CJK_FONT.to_owned());
        // 等宽家族作为兜底追加在末尾，避免代码/数字被中文字体接管。
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .push(CJK_FONT.to_owned());
    } else {
        log::warn!("[font] No CJK font found; Chinese text may render as tofu");
    }

    ctx.set_fonts(fonts);
}
