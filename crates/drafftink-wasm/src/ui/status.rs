//! Status bar — online/offline indicator, sync status, draft/submit status.

use egui::{Color32, Ui};

use crate::app::WasmApp;

/// Render the status bar at the bottom of the window.
pub fn render(ui: &mut Ui, app: &WasmApp, now: f64) {
    ui.horizontal(|ui| {
        // Online / offline indicator
        let (dot_color, label) = if app.online {
            (Color32::from_rgb(0x4C, 0xAF, 0x50), "Online")
        } else {
            (Color32::from_rgb(0xF4, 0x43, 0x36), "Offline")
        };
        ui.painter().circle_filled(
            ui.min_rect().min + egui::vec2(8.0, ui.min_rect().height() / 2.0),
            4.0,
            dot_color,
        );
        ui.add_space(14.0);
        ui.label(label);

        ui.separator();

        // Last sync time
        match app.last_sync_time {
            Some(t) => {
                let elapsed = (now - t).max(0.0) as u64;
                ui.label(format!("Last sync: {elapsed}s ago"));
            }
            None => {
                ui.colored_label(Color32::from_rgb(0x99, 0x99, 0x99), "Last sync: never");
            }
        }

        ui.separator();

        // Draft save status
        let draft_label = if app.draft_saved {
            "Draft: saved"
        } else {
            "Draft: unsaved"
        };
        let draft_color = if app.draft_saved {
            Color32::from_rgb(0x4C, 0xAF, 0x50)
        } else {
            Color32::from_rgb(0xFF, 0x98, 0x00)
        };
        ui.colored_label(draft_color, draft_label);

        ui.separator();

        // Submit status
        if !app.submit_status.is_empty() {
            ui.colored_label(Color32::from_rgb(0x21, 0x96, 0xF3), &app.submit_status);
        } else {
            ui.colored_label(Color32::from_rgb(0x99, 0x99, 0x99), "Submit: idle");
        }

        ui.separator();

        // Sync status (general)
        if !app.sync_status.is_empty() {
            ui.colored_label(Color32::from_rgb(0xAA, 0xAA, 0xAA), &app.sync_status);
        }
    });
}
