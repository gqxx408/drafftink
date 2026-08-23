//! Left sidebar with navigation buttons.
//!
//! Renders a vertical `SidePanel` containing four navigation entries:
//! 备课 (Prepare), 上课 (Teach), 批改 (Grade), 设置 (Settings).
//! Each button shows a Unicode icon and a Chinese label.  The currently
//! active view is highlighted.

use egui::{Color32, Rounding, Vec2};

use crate::app::{AppView, DesktopApp};

/// Render the sidebar as a left `SidePanel`.
pub fn show(app: &mut DesktopApp, ctx: &egui::Context) {
    egui::SidePanel::left("sidebar")
        .resizable(false)
        .exact_width(88.0)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.add_space(8.0);

            // App logo / title area
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);
                ui.heading("DT");
                ui.label(
                    egui::RichText::new("Drafftink")
                        .small()
                        .color(Color32::from_gray(140)),
                );
            });
            ui.add_space(16.0);
            ui.separator();
            ui.add_space(8.0);

            let views = [
                AppView::Prepare,
                AppView::Teach,
                AppView::Grade,
                AppView::Settings,
            ];

            for view in views {
                render_nav_button(app, ui, view);
            }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("v0.1.0")
                        .small()
                        .color(Color32::from_gray(100)),
                );
                ui.add_space(4.0);
            });
        });
}

/// Render a single navigation button.
fn render_nav_button(app: &mut DesktopApp, ui: &mut egui::Ui, view: AppView) {
    let is_active = app.current_view == view;
    let icon = view.icon();
    let label = view.label();

    let (bg, fg) = if is_active {
        (Color32::from_rgb(0x3A, 0x86, 0xFF), Color32::WHITE)
    } else {
        (Color32::TRANSPARENT, Color32::from_gray(180))
    };

    let button_size = Vec2::new(72.0, 64.0);
    let (rect, resp) = ui.allocate_exact_size(button_size, egui::Sense::click());

    // Draw background directly (no Frame needed)
    if bg != Color32::TRANSPARENT {
        ui.painter()
            .rect_filled(rect, Rounding::same(10.0), bg);
    }

    // Draw icon and label centered
    let center = rect.center();
    let painter = ui.painter_at(rect);
    painter.text(
        center - egui::vec2(0.0, 10.0),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(22.0),
        fg,
    );
    painter.text(
        center + egui::vec2(0.0, 14.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(13.0),
        fg,
    );

    if resp.clicked() {
        app.current_view = view;
        app.set_status(format!("切换到{}视图", view.label()));
    }

    // Hover effect
    if resp.hovered() && !is_active {
        ui.painter()
            .rect_filled(rect, Rounding::same(10.0), Color32::from_gray(50));
    }
}
