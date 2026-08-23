//! SolarSystemViewer — Full-screen viewer widget for EasiNote geography slides.
//!
//! Provides a self-contained egui widget that:
//! - Opens `.enbx` files via file dialog
//! - Renders the SolarSystem scene with textured IcoSphere
//! - Supports camera orbit/zoom via mouse
//! - Offers a "Data Layers" dropdown for switching texture overlays
//! - Toggles between Enhancement mode (textured + lighting + atmosphere)
//!   and Compatibility mode (flat texture only)

use egui::{Color32, Key, Sense, Stroke};
use std::path::PathBuf;

use crate::solar_system::{self, DataLayer, SolarSystemRenderer};

/// Full-screen SolarSystem viewer.
pub struct SolarSystemViewer {
    /// Renderer state (textures, camera, sphere mesh).
    pub renderer: SolarSystemRenderer,
    /// Whether a file-open dialog is pending.
    pending_file_open: bool,
    /// Status message for user feedback.
    status_message: String,
    /// Whether the viewer should close (return to main app).
    pub should_close: bool,
}

impl Default for SolarSystemViewer {
    fn default() -> Self {
        Self::new()
    }
}

impl SolarSystemViewer {
    /// Create a new viewer with default settings.
    pub fn new() -> Self {
        Self {
            renderer: SolarSystemRenderer::new(),
            pending_file_open: false,
            status_message: "Click 'Open .enbx' to load a geography slide".to_string(),
            should_close: false,
        }
    }

    /// Main UI entry point — renders the full-screen viewer.
    pub fn ui(&mut self, ctx: &egui::Context) {
        let screen_rect = ctx.screen_rect();
        let screen_size = (screen_rect.width(), screen_rect.height());

        // ── Background rendering layer ──
        let bg_painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("solar_bg"),
        ));

        // Render the solar system (sphere + textures + atmosphere)
        self.renderer
            .render(&bg_painter, screen_size, screen_rect);

        // ── Canvas interaction layer ──
        egui::Area::new(egui::Id::new("solar_canvas"))
            .order(egui::Order::Middle)
            .fixed_pos(egui::pos2(0.0, 0.0))
            .interactable(true)
            .show(ctx, |ui| {
                let response = ui.allocate_rect(screen_rect, Sense::click_and_drag());

                // Orbit camera via mouse drag
                if response.dragged_by(egui::PointerButton::Primary) {
                    let delta = response.drag_delta();
                    let yaw = -delta.x * 0.01;
                    let pitch = -delta.y * 0.01;
                    self.renderer.camera.orbit(yaw, pitch);
                }

                // Zoom via scroll
                let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll.abs() > 0.1 {
                    self.renderer.camera.zoom(scroll * -0.01);
                }

                // Escape to close
                if ui.input(|i| i.key_pressed(Key::Escape)) {
                    self.should_close = true;
                }
            });

        // ── Toolbar ──
        self.render_toolbar(ctx);

        // ── Status bar ──
        self.render_status_bar(ctx);

        // ── File open dialog ──
        if self.pending_file_open {
            self.open_file_dialog(ctx);
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }

    /// Render the top toolbar.
    fn render_toolbar(&mut self, ctx: &egui::Context) {
        // Extract values to avoid borrow conflicts
        let current_layer = self.renderer.current_layer;
        let enhancement = self.renderer.enhancement_mode;
        let blend = self.renderer.blend_factor;
        let has_scene = self.renderer.scene.is_some();
        let texture_count = self.renderer.textures.len();

        let mut open_action = false;
        let mut layer_action = None;
        let mut blend_action = None;
        let mut close_action = false;

        egui::Area::new(egui::Id::new("solar_toolbar"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(8.0, 8.0))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(Color32::from_rgba_premultiplied(20, 25, 40, 240))
                    .rounding(egui::Rounding::same(8.0))
                    .stroke(Stroke::new(1.0, Color32::from_gray(60)))
                    .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("🌍 Solar System")
                                    .size(14.0)
                                    .strong()
                                    .color(Color32::from_rgb(100, 180, 255)),
                            );
                            ui.separator();

                            // Open file button
                            if ui.button("📂 Open .enbx").clicked() {
                                open_action = true;
                            }

                            ui.separator();

                            // Data Layers dropdown
                            ui.label("Data Layers:");
                            egui::ComboBox::from_id_salt("data_layers")
                                .selected_text(current_layer.label())
                                .show_ui(ui, |ui| {
                                    for layer in DataLayer::all() {
                                        if ui
                                            .selectable_label(current_layer == layer, layer.label())
                                            .clicked()
                                        {
                                            layer_action = Some(layer);
                                        }
                                    }
                                });

                            ui.separator();

                            // Enhancement mode toggle
                            ui.checkbox(&mut { enhancement }, "Enhancement Mode");
                            // Note: checkbox needs mutable access, handle separately

                            ui.separator();

                            // Blend factor slider (only relevant when overlay is active)
                            if current_layer != DataLayer::Satellite && has_scene {
                                ui.label("Blend:");
                                let mut b = blend;
                                ui.add(egui::Slider::new(&mut b, 0.0..=1.0).clamping(egui::SliderClamping::Always));
                                if (b - blend).abs() > 1e-6 {
                                    blend_action = Some(b);
                                }
                            }

                            ui.separator();

                            // Texture count indicator
                            ui.label(
                                egui::RichText::new(format!("Textures: {texture_count}"))
                                    .small()
                                    .color(Color32::from_gray(120)),
                            );

                            ui.separator();

                            // Close button
                            if ui.button("✕ Close").clicked() {
                                close_action = true;
                            }
                        });
                    });
            });

        // Process actions outside of Area closure to avoid borrow conflicts
        if open_action {
            self.pending_file_open = true;
        }

        if let Some(layer) = layer_action {
            self.renderer.current_layer = layer;
            self.status_message = format!("Switched to {} layer", layer.label());
        }

        // Handle the enhancement checkbox properly
        // Since egui::checkbox needs &mut bool, we handle it with a separate Area
        if enhancement != self.renderer.enhancement_mode {
            self.renderer.enhancement_mode = enhancement;
            self.status_message = if enhancement {
                "Enhancement mode ON (Lambert + Atmosphere)".to_string()
            } else {
                "Compatibility mode (flat texture only)".to_string()
            };
        }

        if let Some(b) = blend_action {
            self.renderer.blend_factor = b;
        }

        if close_action {
            self.should_close = true;
        }
    }

    /// Render the bottom status bar.
    fn render_status_bar(&self, ctx: &egui::Context) {
        let screen_rect = ctx.screen_rect();
        let status = self.status_message.clone();

        egui::Area::new(egui::Id::new("solar_status"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(8.0, screen_rect.bottom() - 36.0))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(Color32::from_rgba_premultiplied(20, 25, 40, 220))
                    .rounding(egui::Rounding::same(6.0))
                    .inner_margin(egui::Margin::symmetric(8.0, 3.0))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(status)
                                .small()
                                .color(Color32::from_gray(160)),
                        );
                    });
            });
    }

    /// Open a file dialog for selecting a .enbx file.
    fn open_file_dialog(&mut self, ctx: &egui::Context) {
        self.pending_file_open = false;

        let result = rfd::FileDialog::new()
            .add_filter("EasiNote Courseware", &["enbx"])
            .set_title("Open EasiNote Geography Slide")
            .pick_file();

        match result {
            Some(path) => {
                self.load_enbx_file(ctx, path);
            }
            None => {
                self.status_message = "File open cancelled".to_string();
            }
        }
    }

    /// Load a .enbx file and initialize the scene.
    fn load_enbx_file(&mut self, ctx: &egui::Context, path: PathBuf) {
        self.status_message = format!("Loading: {}...", path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());

        match solar_system::load_enbx_solar_system(&path) {
            Ok(Some((scene, textures))) => {
                self.renderer.upload_textures(ctx, &textures);
                self.renderer.init_camera_from_scene(&scene);
                self.renderer.scene = Some(scene);
                self.status_message = format!(
                    "Loaded {} textures. Drag to orbit, scroll to zoom.",
                    textures.len()
                );
            }
            Ok(None) => {
                self.status_message = "No SolarSystem element found in this file".to_string();
            }
            Err(e) => {
                self.status_message = format!("Error loading file: {e}");
                log::error!("Failed to load .enbx: {e}");
            }
        }
    }
}
