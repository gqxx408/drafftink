//! Plugin installation confirmation dialog.
//!
//! Shows the plugin's metadata, signature status, and requested permissions.
//! The user must explicitly approve or deny the installation.

use eframe::egui;
use drafftink_core::plugin::api::Permission;
use drafftink_core::plugin::api::SignatureStatus;

pub struct InstallDialog {
    pub open: bool,
    pub plugin_name: String,
    pub developer: String,
    pub version: String,
    pub description: String,
    pub permissions: Vec<Permission>,
    pub sig_status: SignatureStatus,
    /// None = undecided, Some(true) = approved, Some(false) = rejected
    pub choice: Option<bool>,
}

impl InstallDialog {
    pub fn new(
        name: &str,
        dev: &str,
        ver: &str,
        desc: &str,
        perms: Vec<Permission>,
        sig: SignatureStatus,
    ) -> Self {
        Self {
            open: true,
            plugin_name: name.into(),
            developer: dev.into(),
            version: ver.into(),
            description: desc.into(),
            permissions: perms,
            sig_status: sig,
            choice: None,
        }
    }

    /// Draw the modal dialog. Returns the user's choice once made.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<bool> {
        if !self.open {
            return self.choice;
        }

        let mut close = false;

        egui::Window::new("插件安装确认")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_width(380.0);

                // Title
                ui.heading(&self.plugin_name);
                ui.label(format!("v{}  ·  {}", self.version, self.developer));
                if !self.description.is_empty() {
                    ui.label(&self.description);
                }

                ui.separator();

                // Signature status
                let sig_text;
                let sig_color;
                match self.sig_status {
                    SignatureStatus::Verified => {
                        sig_text = "✓ 已验证（官方/受信任）";
                        sig_color = egui::Color32::GREEN;
                    }
                    SignatureStatus::SelfSigned => {
                        sig_text = "⚠ 自签名（社区开发者）";
                        sig_color = egui::Color32::YELLOW;
                    }
                    SignatureStatus::Untrusted => {
                        sig_text = "✗ 签名无效 — 可能被篡改";
                        sig_color = egui::Color32::RED;
                    }
                    SignatureStatus::NoSignature => {
                        sig_text = "✗ 无签名 — 来源不可验证";
                        sig_color = egui::Color32::RED;
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("签名状态: ");
                    ui.colored_label(sig_color, sig_text);
                });

                ui.separator();

                // Permissions
                ui.label("该插件请求以下权限：");
                egui::ScrollArea::vertical()
                    .max_height(120.0)
                    .show(ui, |ui| {
                        if self.permissions.is_empty() {
                            ui.label("  (无特殊权限)");
                        }
                        for perm in &self.permissions {
                            let (icon, desc) = match perm {
                                Permission::ReadFiles => ("📖", "读取文件"),
                                Permission::WriteFiles => ("✏️", "写入文件"),
                                Permission::NetworkAccess => ("🌐", "网络访问"),
                                Permission::FullScreen => ("🖥️", "全屏覆盖"),
                                Permission::SystemInfo => ("ℹ️", "系统信息"),
                            };
                            ui.horizontal(|ui| {
                                ui.label(icon);
                                ui.label(desc);
                            });
                        }
                    });

                // Warning for untrusted/unsigned
                if matches!(self.sig_status, SignatureStatus::Untrusted | SignatureStatus::NoSignature) {
                    ui.add_space(4.0);
                    ui.colored_label(
                        egui::Color32::RED,
                        "⚠ 此插件未经签名验证，可能包含恶意代码。\n请确认从可信来源获取。",
                    );
                }

                ui.separator();

                // Buttons
                ui.horizontal(|ui| {
                    if ui.button("❌ 取消").clicked() {
                        self.choice = Some(false);
                        close = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new("✅ 确认安装").fill(
                                egui::Color32::from_rgb(40, 120, 40),
                            ))
                            .clicked()
                        {
                            self.choice = Some(true);
                            close = true;
                        }
                    });
                });
            });

        if close {
            self.open = false;
        }

        self.choice
    }
}
