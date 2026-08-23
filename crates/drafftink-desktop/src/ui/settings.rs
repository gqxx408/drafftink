//! Settings view (设置).
//!
//! Sections:
//! - Backend URL configuration
//! - Login form (username, password)
//! - Plugin management (list, enable/disable, install)
//! - Theme confirmation (dark)
//! - About section

use egui::Color32;

use crate::app::DesktopApp;

/// Render the settings view inside the central panel.
pub fn show(app: &mut DesktopApp, ui: &mut egui::Ui) {
    ui.heading("设置");
    ui.separator();
    ui.add_space(8.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        backend_section(app, ui);
        ui.add_space(16.0);
        ui.separator();
        login_section(app, ui);
        ui.add_space(16.0);
        ui.separator();
        plugin_section(app, ui);
        ui.add_space(16.0);
        ui.separator();
        theme_section(ui);
        ui.add_space(16.0);
        ui.separator();
        about_section(ui);
    });
}

// ════════════════════════════════════════════════════════════════════════════
//  Backend URL
// ════════════════════════════════════════════════════════════════════════════

fn backend_section(app: &mut DesktopApp, ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("后端服务器").strong().size(16.0));
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label("URL:");
        ui.add(
            egui::TextEdit::singleline(&mut app.backend_url)
                .hint_text("http://localhost:3000")
                .desired_width(300.0),
        );
    });

    ui.add_space(4.0);
    if ui.button("测试连接").clicked() {
        // Simulate connection test
        app.set_status(format!("正在连接 {}…", app.backend_url));
        // In a real app, this would make an HTTP request.
        // For the MVP, we just set a status.
        if app.backend_url.starts_with("http") {
            app.set_status(format!("连接 {} 成功（模拟）", app.backend_url));
        } else {
            app.set_status("URL 必须以 http:// 或 https:// 开头");
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  Login
// ════════════════════════════════════════════════════════════════════════════

fn login_section(app: &mut DesktopApp, ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("登录").strong().size(16.0));
    ui.add_space(4.0);

    if app.jwt_token.is_some() {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("已登录")
                    .color(Color32::from_rgb(0x4C, 0xAF, 0x50)),
            );
            if ui.button("退出登录").clicked() {
                app.jwt_token = None;
                app.login_username.clear();
                app.login_password.clear();
                app.set_status("已退出登录");
            }
        });
        return;
    }

    ui.horizontal(|ui| {
        ui.label("用户名:");
        ui.add(
            egui::TextEdit::singleline(&mut app.login_username)
                .hint_text("教师用户名")
                .desired_width(200.0),
        );
    });
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label("密码:");
        ui.add(
            egui::TextEdit::singleline(&mut app.login_password)
                .hint_text("密码")
                .password(true)
                .desired_width(200.0),
        );
    });
    ui.add_space(8.0);

    if ui.button("登录").clicked() {
        if app.login_username.is_empty() || app.login_password.is_empty() {
            app.set_status("请输入用户名和密码");
        } else {
            // Simulate login — in a real app, POST to backend /api/auth/login
            let token = format!("jwt_{}", app.login_username);
            app.jwt_token = Some(token);
            app.set_status(format!("登录成功: {}", app.login_username));
            app.login_password.clear();
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  Plugin Management
// ════════════════════════════════════════════════════════════════════════════

fn plugin_section(app: &mut DesktopApp, ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("插件管理").strong().size(16.0));
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label(format!("插件目录: {:?}", app.plugin_manager.plugin_dir));
    });
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        if ui.button("扫描插件").clicked() {
            match app.plugin_manager.scan_plugins() {
                Ok(count) => {
                    app.set_status(format!("扫描完成: 发现 {count} 个插件"));
                }
                Err(e) => {
                    app.set_status(format!("扫描失败: {e}"));
                }
            }
        }
        if ui.button("安装插件…").clicked() {
            handle_install_plugin(app);
        }
    });
    ui.add_space(8.0);

    // Plugin list
    let plugin_count = app.plugin_manager.plugins.len();
    if plugin_count == 0 {
        ui.label(
            egui::RichText::new("暂无已加载插件")
                .color(Color32::from_gray(140)),
        );
    } else {
        ui.label(format!("已加载 {plugin_count} 个插件:"));
        ui.add_space(4.0);

        // Collect names first to avoid borrow issues
        let names: Vec<String> = app
            .plugin_manager
            .plugins
            .iter()
            .map(|p| p.name.clone())
            .collect();

        for name in &names {
            ui.horizontal(|ui| {
                let plugin = app
                    .plugin_manager
                    .plugins
                    .iter()
                    .find(|p| &p.name == name);

                if let Some(plugin) = plugin {
                    let enabled = plugin.enabled;
                    let status = if enabled { "启用" } else { "禁用" };
                    let status_color = if enabled {
                        Color32::from_rgb(0x4C, 0xAF, 0x50)
                    } else {
                        Color32::from_gray(120)
                    };

                    ui.label(format!("{} v{}", plugin.name, plugin.version));
                    ui.label(
                        egui::RichText::new(status)
                            .color(status_color)
                            .small(),
                    );

                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            let btn_text = if enabled { "禁用" } else { "启用" };
                            if ui.small_button(btn_text).clicked() {
                                if let Err(e) =
                                    app.plugin_manager.toggle_plugin(name)
                                {
                                    app.set_status(format!("切换失败: {e}"));
                                }
                            }
                            if ui.small_button("卸载").clicked() {
                                if let Err(e) =
                                    app.plugin_manager.unload_plugin(name)
                                {
                                    app.set_status(format!("卸载失败: {e}"));
                                }
                            }
                        },
                    );
                }
            });
        }
    }
}

/// Handle plugin installation via file dialog.
fn handle_install_plugin(app: &mut DesktopApp) {
    let dialog = app
        .file_dialog()
        .add_filter("插件", &["dll", "wasm"])
        .set_title("选择插件文件");

    if let Some(picked) = dialog.pick_file() {
        match app.plugin_manager.load_plugin(&picked) {
            Ok(name) => {
                app.set_status(format!("插件已加载: {name}"));
            }
            Err(e) => {
                app.set_status(format!("加载失败: {e}"));
            }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  Theme
// ════════════════════════════════════════════════════════════════════════════

fn theme_section(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("主题").strong().size(16.0));
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("当前主题: 深色 (Dark)")
                .color(Color32::from_rgb(0x3A, 0x86, 0xFF)),
        );
    });
    ui.label(
        egui::RichText::new("深色主题已强制启用，确保在所有系统主题下保持一致的暗色 UI。")
            .small()
            .color(Color32::from_gray(140)),
    );
}

// ════════════════════════════════════════════════════════════════════════════
//  About
// ════════════════════════════════════════════════════════════════════════════

fn about_section(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("关于").strong().size(16.0));
    ui.add_space(4.0);

    ui.vertical(|ui| {
        ui.label("Drafftink Desktop");
        ui.label(
            egui::RichText::new("教师桌面应用 — 备课 · 上课 · 批改")
                .small()
                .color(Color32::from_gray(160)),
        );
        ui.add_space(4.0);
        ui.label(format!("版本: {}", env!("CARGO_PKG_VERSION")));
        ui.label(format!("许可: {}", env!("CARGO_PKG_LICENSE")));
        ui.label("纯 Rust 实现，无 C 依赖");
        ui.label("基于 egui / eframe / wgpu");
    });
}
