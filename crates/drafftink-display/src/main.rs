#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod annotation;
mod app;
mod interaction;
mod log_setup;
mod multi_page;
mod physics;
mod render;
mod workshop;

use drafftink_core::plugin::api::DummyContext;

fn load_cjk_font() -> Option<(Vec<u8>, &'static str)> {
    let candidates: &[(&str, &str)] = &[
        ("C:\\Windows\\Fonts\\msyh.ttf", "Microsoft YaHei"),
        ("C:\\Windows\\Fonts\\msyhbd.ttf", "Microsoft YaHei Bold"),
        ("C:\\Windows\\Fonts\\simhei.ttf", "SimHei"),
        ("C:\\Windows\\Fonts\\simkai.ttf", "KaiTi"),
        ("C:\\Windows\\Fonts\\simfang.ttf", "FangSong"),
        ("C:\\Windows\\Fonts\\Deng.ttf", "DengXian"),
        ("C:\\Windows\\Fonts\\Dengb.ttf", "DengXian Bold"),
        ("C:\\Windows\\Fonts\\msyh.ttc", "Microsoft YaHei (ttc)"),
        ("C:\\Windows\\Fonts\\simsun.ttc", "SimSun (ttc)"),
        (
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            "WenQuanYi",
        ),
        (
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "Noto CJK",
        ),
        ("/System/Library/Fonts/PingFang.ttc", "PingFang"),
        ("/System/Library/Fonts/STHeiti Light.ttc", "STHeiti"),
    ];
    for (path, name) in candidates {
        match std::fs::read(path) {
            Ok(bytes) if bytes.len() > 1024 => {
                log::info!(
                    "[font] Loaded CJK font: {} ({:.1} KB, path={})",
                    name,
                    bytes.len() as f64 / 1024.0,
                    path,
                );
                return Some((bytes, name));
            }
            Ok(_) => log::warn!(
                "[font] Skipping {} (too small, likely not a valid font)",
                path
            ),
            Err(_) => {}
        }
    }
    log::warn!("[font] No CJK font found; all Chinese text will render as tofu");
    None
}

fn install_cjk_fonts(ctx: &egui::Context) {
    let Some((font_data, name)) = load_cjk_font() else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    let key = "cjk".to_owned();
    fonts
        .font_data
        .insert(key.clone(), egui::FontData::from_owned(font_data).into());
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .push(key.clone());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .push(key);
    log::info!("[font] Installed CJK fallback: {}", name);
    ctx.set_fonts(fonts);
}

fn main() {
    // ── Logging: file in ./logs/ + terminal if available ──
    let _log_path = log_setup::init_logger();

    let args: Vec<String> = std::env::args().collect();

    let (doc, doc_path) = if args.len() >= 2 {
        let path = args[1].clone();
        let path_lower = path.to_lowercase();
        let loaded = if path_lower.ends_with(".enbx") || path_lower.ends_with(".enbxz") {
            // Use format_enbx loader directly for .enbx files
            match std::fs::read(&path) {
                Ok(data) => match format_enbx::loader::load_enbx(&data, &DummyContext) {
                    Ok(doc) => {
                        log::info!("[display] Loaded .enbx via built-in loader: {}", path);
                        Ok(doc)
                    }
                    Err(e) => {
                        log::error!("[display] .enbx load failed: {}", e);
                        Err(e)
                    }
                },
                Err(e) => Err(format!("read file failed: {}", e)),
            }
        } else {
            drafftink_core::document::load_document(&path).map_err(|e| e.to_string())
        };

        match loaded {
            Ok(doc) => {
                log::info!(
                    "[display] Loaded: {} ({} pages, page_size={:?})",
                    path,
                    doc.pages.len(),
                    doc.page_size
                );
                (doc, Some(path))
            }
            Err(e) => {
                log::error!("[display] Failed to load '{}': {}", path, e);
                (drafftink_core::model::CoursewareDoc::empty(), None)
            }
        }
    } else {
        log::info!("[display] No file argument, starting with empty canvas.");
        (drafftink_core::model::CoursewareDoc::empty(), None)
    };

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_fullscreen(true)
            .with_title("Drafftink 授课端"),
        // ── CTO 指令: 强制硬件 GPU，禁止软件回退 ──
        hardware_acceleration: eframe::HardwareAcceleration::Required,
        renderer: eframe::Renderer::Wgpu,
        // V-Sync 已在 NativeOptions 默认开启，这里显式确认
        vsync: true,
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            // 排除 GL backend，避免 llvmpipe / Software 渲染器
            supported_backends: wgpu::Backends::PRIMARY,
            // 强制高性能 GPU（独显优先）
            power_preference: wgpu::PowerPreference::HighPerformance,
            // 强制 V-Sync: Fifo = 60fps 锁死，不空转
            present_mode: wgpu::PresentMode::Fifo,
            ..Default::default()
        },
        ..Default::default()
    };

    if let Err(e) = eframe::run_native(
        "Drafftink Display",
        native_options,
        Box::new(move |cc| {
            install_cjk_fonts(&cc.egui_ctx);

            // ── 强制深色主题：无论 Windows 系统是浅色还是深色模式，始终保持深色 UI ──
            cc.egui_ctx.set_theme(egui::ThemePreference::Dark);

            // ── CTO 指令: 验证 GPU 后端 ──
            if let Some(ref rs) = cc.wgpu_render_state {
                let info = rs.adapter.get_info();
                log::info!(
                    "[gpu] Adapter: name=\"{}\" backend={:?} device_type={:?} vendor={} driver=\"{}\"",
                    info.name, info.backend, info.device_type, info.vendor, info.driver
                );

                // 检查是否回退到了软件渲染器
                let is_software = info.device_type == wgpu::DeviceType::Cpu
                    || info.name.to_lowercase().contains("llvmpipe")
                    || info.name.to_lowercase().contains("software")
                    || info.name.to_lowercase().contains("microsoft basic render");

                if is_software {
                    log::error!(
                        "[gpu] CRITICAL: Software/CPU renderer detected! \
                         Expected hardware GPU (Vulkan/Metal/DX12). \
                         Check your graphics drivers. \
                         Adapter: name=\"{}\" backend={:?} device_type={:?}",
                        info.name,
                        info.backend,
                        info.device_type
                    );
                } else {
                    log::info!(
                        "[gpu] Hardware GPU confirmed: backend={:?} device_type={:?}",
                        info.backend,
                        info.device_type
                    );
                }

                // 打印所有可用适配器，帮助诊断
                for (i, adapter) in rs.available_adapters.iter().enumerate() {
                    let a = adapter.get_info();
                    log::info!(
                        "[gpu] Available adapter #{}: name=\"{}\" backend={:?} device_type={:?}",
                        i,
                        a.name,
                        a.backend,
                        a.device_type
                    );
                }
            } else {
                log::warn!("[gpu] wgpu_render_state is None — rendering backend may not be wgpu");
            }

            Ok(Box::new(app::DisplayApp::new(doc, doc_path, None)))
        }),
    ) {
        log::error!("Display fatal: {}", e);
    }
}
