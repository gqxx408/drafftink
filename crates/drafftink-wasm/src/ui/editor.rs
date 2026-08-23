//! Homework editor UI.
//!
//! Renders the text input area, a simple drawing canvas (pen tool),
//! submit / save-draft buttons, and a QR / parent-scan panel.

use egui::{Color32, Pos2, Sense, Stroke, Ui};

use crate::app::WasmApp;

// ════════════════════════════════════════════════════════════════════════════
//  Constants
// ════════════════════════════════════════════════════════════════════════════

const PEN_COLOR: Color32 = Color32::from_rgb(0x21, 0x96, 0xF3);
const PEN_WIDTH: f32 = 2.5;
const CANVAS_BG: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
const CANVAS_BORDER: Color32 = Color32::from_rgb(0xCC, 0xCC, 0xCC);

/// Minimum drawing canvas height.
const CANVAS_MIN_HEIGHT: f32 = 200.0;

// ════════════════════════════════════════════════════════════════════════════
//  Public API
// ════════════════════════════════════════════════════════════════════════════

/// Render the homework editor into the central panel.
pub fn render(ui: &mut Ui, app: &mut WasmApp) {
    ui.vertical(|ui| {
        render_header(ui, app);
        ui.separator();

        // Two-column layout: editor on the left, tools/QR on the right
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_min_width(ui.available_width() * 0.6);
                render_text_editor(ui, app);
                ui.add_space(8.0);
                render_drawing_canvas(ui, app);
            });

            ui.vertical(|ui| {
                render_action_buttons(ui, app);
                ui.add_space(8.0);
                render_qr_panel(ui, app);
            });
        });
    });
}

// ════════════════════════════════════════════════════════════════════════════
//  Sub-views
// ════════════════════════════════════════════════════════════════════════════

/// Header showing the homework ID.
fn render_header(ui: &mut Ui, app: &WasmApp) {
    ui.horizontal(|ui| {
        ui.heading("Drafftink Homework Editor");
        if let Some(hw_id) = app.homework_id {
            ui.label(format!("Homework: {hw_id}"));
        } else {
            ui.colored_label(Color32::from_rgb(0xFF, 0x99, 0x00), "No homework ID in URL");
        }
    });
}

/// Multi-line text editor for the student's answer.
fn render_text_editor(ui: &mut Ui, app: &mut WasmApp) {
    ui.label("Answer:");
    let resp = ui.add(
        egui::TextEdit::multiline(&mut app.answer_text)
            .desired_width(f32::INFINITY)
            .desired_rows(4)
            .hint_text("Type your answer here..."),
    );
    if resp.changed() {
        app.draft_saved = false;
    }
}

/// Simple drawing canvas with a pen tool.
fn render_drawing_canvas(ui: &mut Ui, app: &mut WasmApp) {
    ui.horizontal(|ui| {
        ui.label("Annotations:");
        if ui.button("Clear").clicked() {
            app.strokes.clear();
            app.current_stroke.clear();
            app.draft_saved = false;
        }
    });

    let available_height = (ui.available_height() - 30.0).max(CANVAS_MIN_HEIGHT);
    let (canvas_rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), available_height), Sense::drag());

    // Draw canvas background and border
    let painter = ui.painter_at(canvas_rect);
    painter.rect_filled(canvas_rect, 0.0, CANVAS_BG);
    painter.rect_stroke(canvas_rect, 0.0, Stroke::new(1.0_f32, CANVAS_BORDER));

    // Handle pen input
    if response.dragged() {
        if let Some(pos) = response.hover_pos() {
            // Clamp point to canvas
            let clamped = clamp_to_rect(pos, canvas_rect);
            if app.current_stroke.is_empty()
                || app
                    .current_stroke
                    .last()
                    .map(|last| last.distance(clamped) > 1.0)
                    .unwrap_or(true)
            {
                app.current_stroke.push(clamped);
            }
        }
    } else if !app.current_stroke.is_empty() {
        // Stroke finished
        app.strokes.push(std::mem::take(&mut app.current_stroke));
        app.draft_saved = false;
    }

    // Render completed strokes
    let stroke = Stroke::new(PEN_WIDTH, PEN_COLOR);
    for s in &app.strokes {
        draw_stroke(&painter, s, stroke);
    }
    // Render current stroke
    if !app.current_stroke.is_empty() {
        draw_stroke(&painter, &app.current_stroke, stroke);
    }
}

/// Draw a single stroke (polyline) on the painter.
fn draw_stroke(painter: &egui::Painter, points: &[Pos2], stroke: Stroke) {
    if points.len() < 2 {
        if let Some(p) = points.first() {
            painter.circle_filled(*p, PEN_WIDTH / 2.0, PEN_COLOR);
        }
        return;
    }
    for w in points.windows(2) {
        painter.line_segment([w[0], w[1]], stroke);
    }
}

/// Clamp a point to stay within a rectangle.
fn clamp_to_rect(pos: Pos2, rect: egui::Rect) -> Pos2 {
    Pos2::new(
        pos.x.clamp(rect.min.x, rect.max.x),
        pos.y.clamp(rect.min.y, rect.max.y),
    )
}

/// Submit and save-draft buttons.
fn render_action_buttons(ui: &mut Ui, app: &mut WasmApp) {
    ui.set_min_width(200.0);
    ui.vertical(|ui| {
        if ui.button("Submit Homework").clicked() {
            app.submit();
        }
        if ui.button("Save Draft").clicked() {
            app.save_draft();
        }
        ui.checkbox(&mut app.show_qr, "Show Parent QR");

        if !app.submit_status.is_empty() {
            ui.add_space(4.0);
            ui.colored_label(Color32::from_rgb(0x4C, 0xAF, 0x50), &app.submit_status);
        }
    });
}

/// QR / parent-scan panel showing homework status as text.
fn render_qr_panel(ui: &mut Ui, app: &mut WasmApp) {
    if !app.show_qr {
        return;
    }
    ui.set_min_width(200.0);
    ui.group(|ui| {
        ui.label("Parent Scan Info");
        ui.separator();

        let mut qr_text = app.qr_text();

        ui.add(
            egui::TextEdit::multiline(&mut qr_text)
                .desired_width(f32::INFINITY)
                .desired_rows(6)
                .font(egui::TextStyle::Monospace),
        );

        ui.add_space(4.0);
        ui.small("(QR rendering — future work)");
    });
}
