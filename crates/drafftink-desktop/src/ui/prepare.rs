//! Lesson preparation view (备课).
//!
//! Layout:
//! ```text
//! ┌──────────────────────────────────────────────────────┐
//! │  Toolbar: [Select][Text][Shape][3D][Func][Map] │ [Import][Export][Save] │
//! ├──────────────────────────────────────────┬───────────┤
//! │                                          │  Slides   │
//! │            Canvas Area                   │  ┌───┐    │
//! │   (element list / placeholder)           │  │ 1 │    │
//! │                                          │  ├───┤    │
//! │                                          │  │ 2 │    │
//! │                                          │  └───┘    │
//! └──────────────────────────────────────────┴───────────┘
//! ```

use std::path::PathBuf;

use drafftink_core::element::{Element, ElementData};
use egui::{Color32, Vec2};

use crate::app::{DesktopApp, PrepareTool};
use crate::enbx;

/// Render the prepare view inside the central panel.
pub fn show(app: &mut DesktopApp, ui: &mut egui::Ui) {
    // ── Top toolbar ──
    egui::TopBottomPanel::top("prepare_toolbar")
        .exact_height(44.0)
        .show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.label(egui::RichText::new("工具:").strong());

                let tools = [
                    (PrepareTool::Select, "选择"),
                    (PrepareTool::Text, "文本"),
                    (PrepareTool::Shape, "图形"),
                    (PrepareTool::ThreeD, "3D"),
                    (PrepareTool::Function, "函数"),
                    (PrepareTool::MindMap, "思维导图"),
                ];

                for (tool, label) in tools {
                    let selected = app.active_tool == tool;
                    let btn = egui::SelectableLabel::new(selected, label);
                    if ui.add(btn).clicked() {
                        app.active_tool = tool;
                        app.set_status(format!("工具: {label}"));
                    }
                }

                ui.separator();

                // File operations
                if ui.button("导入 .enbx").clicked() {
                    handle_import_enbx(app);
                }
                if ui.button("导出 .enbx").clicked() {
                    handle_export_enbx(app);
                }
                if ui.button("保存 .drftx").clicked() {
                    handle_save_drftx(app);
                }

                ui.separator();

                if ui.button("+ 新增页").clicked() {
                    app.slides.push(Vec::new());
                    app.selected_slide = app.slides.len() - 1;
                    app.set_status("新增空白页");
                }
            });
        });

    // ── Right panel: slide list ──
    egui::SidePanel::right("prepare_slides")
        .resizable(true)
        .exact_width(160.0)
        .show_inside(ui, |ui| {
            ui.heading("页面");
            ui.separator();

            let slide_count = app.slides.len();
            for i in 0..slide_count {
                let is_current = app.selected_slide == i;
                let label = format!("页 {i}");
                let resp = ui.add_sized(
                    Vec2::new(140.0, 40.0),
                    egui::SelectableLabel::new(is_current, &label),
                );
                if resp.clicked() {
                    app.selected_slide = i;
                    app.set_status(format!("切换到页 {i}"));
                }
            }
        });

    // ── Central canvas area ──
    egui::CentralPanel::default().show_inside(ui, |ui| {
        let (rect, _) = ui.allocate_at_least(
            ui.available_size(),
            egui::Sense::click_and_drag(),
        );

        // Draw canvas background
        ui.painter().rect_filled(
            rect,
            egui::Rounding::same(4.0),
            Color32::from_rgb(0x1E, 0x1E, 0x1E),
        );

        // Draw canvas border
        ui.painter().rect_stroke(
            rect,
            egui::Rounding::same(4.0),
            egui::Stroke::new(1.0_f32, Color32::from_gray(60)),
        );

        // Show elements as a list (MVP placeholder)
        let elements = current_elements(app);
        if elements.is_empty() {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "空白画布 — 使用上方工具添加元素，或导入 .enbx 课件",
                egui::FontId::proportional(16.0),
                Color32::from_gray(120),
            );
        } else {
            ui.painter().text(
                rect.left_top() + egui::vec2(12.0, 12.0),
                egui::Align2::LEFT_TOP,
                format!("当前页 {} 个元素", elements.len()),
                egui::FontId::proportional(14.0),
                Color32::from_gray(160),
            );

            // Display each element's type as a label
            let mut y = rect.top() + 40.0;
            for elem in elements.iter() {
                ui.painter().text(
                    egui::pos2(rect.left() + 12.0, y),
                    egui::Align2::LEFT_TOP,
                    format!("- {} ({})", elem.element_type(), elem.id()),
                    egui::FontId::proportional(13.0),
                    Color32::from_gray(180),
                );
                y += 20.0;
                if y > rect.bottom() - 20.0 {
                    break;
                }
            }
        }

        // Handle canvas click for adding elements (MVP)
        if ui.ui_contains_pointer() {
            let resp = ui.interact(rect, ui.id().with("canvas"), egui::Sense::click());
            if resp.clicked() {
                handle_canvas_click(app, resp.interact_pointer_pos());
            }
        }
    });
}

/// Get the elements for the currently selected slide.
fn current_elements(app: &DesktopApp) -> &[ElementData] {
    if app.selected_slide < app.slides.len() {
        &app.slides[app.selected_slide]
    } else {
        &app.elements
    }
}

/// Handle a click on the canvas — add an element based on the active tool.
fn handle_canvas_click(app: &mut DesktopApp, pos: Option<egui::Pos2>) {
    let Some(pos) = pos else {
        return;
    };

    use drafftink_core::model::{BaseElement, ShapeElement, ShapeType, TextElement};

    let base = BaseElement {
        position: [pos.x, pos.y],
        size: [200.0, 60.0],
        ..Default::default()
    };

    let elem = match app.active_tool {
        PrepareTool::Select => return,
        PrepareTool::Text => ElementData::Text(TextElement {
            base,
            text: "双击编辑文本".to_string(),
            font_size: 24.0,
            font_family: "sans".to_string(),
        }),
        PrepareTool::Shape => ElementData::Shape(ShapeElement {
            base,
            shape_type: ShapeType::Rectangle,
            has_start_arrow: false,
            has_end_arrow: false,
            scale_y: 0.0,
        }),
        PrepareTool::ThreeD => ElementData::geometry(base, serde_json::json!({"type": "cube"})),
        PrepareTool::Function => ElementData::formula(base, "sin(x)"),
        PrepareTool::MindMap => {
            ElementData::mindmap(base, "tree", serde_json::json!({"nodes": []}))
        }
    };

    if app.selected_slide < app.slides.len() {
        app.slides[app.selected_slide].push(elem);
    } else {
        app.elements.push(elem);
    }
    app.set_status("已添加元素");
}

// ════════════════════════════════════════════════════════════════════════════
//  File operations
// ════════════════════════════════════════════════════════════════════════════

/// Handle .enbx import via file dialog.
fn handle_import_enbx(app: &mut DesktopApp) {
    let dialog = app
        .file_dialog()
        .add_filter("Seewo 课件", &["enbx", "enbxz"])
        .set_title("导入 .enbx 课件");

    if let Some(picked) = dialog.pick_file() {
        app.set_status(format!("正在导入: {}", picked.display()));

        match enbx::import_enbx(&picked) {
            Ok(elements) => {
                let count = elements.len();
                app.current_file = Some(PathBuf::from(&picked));
                // Replace current slide's elements with imported ones
                if app.selected_slide < app.slides.len() {
                    app.slides[app.selected_slide] = elements;
                } else {
                    app.elements = elements;
                }
                app.set_status(format!("导入成功: {count} 个元素"));
            }
            Err(e) => {
                log::error!("[prepare] Import failed: {e}");
                app.set_status(format!("导入失败: {e}"));
            }
        }
    }
}

/// Handle .enbx export via file dialog.
fn handle_export_enbx(app: &mut DesktopApp) {
    let elements: Vec<ElementData> = if app.selected_slide < app.slides.len() {
        app.slides[app.selected_slide].clone()
    } else {
        app.elements.clone()
    };

    if elements.is_empty() {
        app.set_status("当前页没有元素可导出");
        return;
    }

    let dialog = app
        .file_dialog()
        .add_filter("Seewo 课件", &["enbx"])
        .set_title("导出为 .enbx")
        .set_file_name("export.enbx");

    if let Some(path) = dialog.save_file() {
        let path = ensure_extension(path, "enbx");
        app.set_status(format!("正在导出: {}", path.display()));

        match enbx::export_enbx(&elements, &path) {
            Ok(()) => {
                app.set_status(format!("导出成功: {}", path.display()));
            }
            Err(e) => {
                log::error!("[prepare] Export failed: {e}");
                app.set_status(format!("导出失败: {e}"));
            }
        }
    }
}

/// Handle .drftx save via file dialog.
fn handle_save_drftx(app: &mut DesktopApp) {
    let elements: Vec<ElementData> = if app.selected_slide < app.slides.len() {
        app.slides[app.selected_slide].clone()
    } else {
        app.elements.clone()
    };

    let dialog = app
        .file_dialog()
        .add_filter("Drafftink 作业", &["drftx"])
        .set_title("保存为 .drftx")
        .set_file_name("lesson.drftx");

    if let Some(path) = dialog.save_file() {
        let path = ensure_extension(path, "drftx");
        app.set_status(format!("正在保存: {}", path.display()));

        // Serialise elements as JSON for the MVP .drftx format.
        match serde_json::to_string_pretty(&elements) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json.as_bytes()) {
                    log::error!("[prepare] Save failed: {e}");
                    app.set_status(format!("保存失败: {e}"));
                } else {
                    app.current_file = Some(path.clone());
                    app.set_status(format!("保存成功: {}", path.display()));
                }
            }
            Err(e) => {
                app.set_status(format!("序列化失败: {e}"));
            }
        }
    }
}

/// Ensure a path has the given extension.
fn ensure_extension(path: PathBuf, ext: &str) -> PathBuf {
    if path.extension().is_none() {
        path.with_extension(ext)
    } else {
        path
    }
}
