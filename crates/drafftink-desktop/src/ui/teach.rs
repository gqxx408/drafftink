//! Teaching / presentation view (上课).
//!
//! Layout:
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │  [Pen][Highlighter][Eraser]  │  Sync: ●  │  [Quick Quiz]     │
//! ├──────────────────────────────────────────────┬───────────────┤
//! │                                              │  Statistics   │
//! │           Fullscreen Canvas                  │  Students: 42 │
//! │         (rendered from elements)             │  Response: 78%│
//! │                                              │  ┌──────────┐ │
//! │                                              │  │ Quiz     │ │
//! │                                              │  │ Panel    │ │
//! │                                              │  └──────────┘ │
//! └──────────────────────────────────────────────┴───────────────┘
//! ```

use drafftink_core::element::Element;
use egui::{Color32, Vec2};

use crate::app::{AnnotationTool, DesktopApp};

/// Render the teach view inside the central panel.
pub fn show(app: &mut DesktopApp, ui: &mut egui::Ui) {
    // ── Top toolbar ──
    egui::TopBottomPanel::top("teach_toolbar")
        .exact_height(44.0)
        .show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.label(egui::RichText::new("批注:").strong());

                let tools = [
                    (AnnotationTool::Pen, "钢笔"),
                    (AnnotationTool::Highlighter, "荧光笔"),
                    (AnnotationTool::Eraser, "橡皮擦"),
                ];

                for (tool, label) in tools {
                    let selected = app.annotation_tool == tool;
                    if ui
                        .add(egui::SelectableLabel::new(selected, label))
                        .clicked()
                    {
                        app.annotation_tool = tool;
                        app.set_status(format!("批注工具: {label}"));
                    }
                }

                ui.separator();

                // Blackboard sync indicator
                let (dot_color, sync_text) = if app.blackboard_synced {
                    (Color32::from_rgb(0x4C, 0xAF, 0x50), "黑板同步: 已连接")
                } else {
                    (Color32::from_rgb(0xF4, 0x43, 0x36), "黑板同步: 未连接")
                };
                ui.horizontal(|ui| {
                    ui.painter()
                        .circle_filled(ui.min_rect().right_center(), 5.0, dot_color);
                    ui.label(sync_text);
                });

                if ui.button("切换同步").clicked() {
                    app.blackboard_synced = !app.blackboard_synced;
                    let state = if app.blackboard_synced {
                        "已连接"
                    } else {
                        "已断开"
                    };
                    app.set_status(format!("黑板同步{state}"));
                }

                ui.separator();

                if ui.button("快速测验").clicked() {
                    app.set_status("发起快速测验…");
                    // Simulate student responses
                    app.student_count = 42;
                    app.response_rate = 0.0;
                }
            });
        });

    // ── Right panel: real-time statistics ──
    egui::SidePanel::right("teach_stats")
        .resizable(true)
        .exact_width(200.0)
        .show_inside(ui, |ui| {
            ui.heading("实时统计");
            ui.separator();

            ui.vertical(|ui| {
                ui.add_space(8.0);

                // Student count
                ui.horizontal(|ui| {
                    ui.label("在线学生:");
                    ui.monospace(
                        egui::RichText::new(format!("{}", app.student_count))
                            .strong()
                            .size(20.0),
                    );
                });

                ui.add_space(8.0);

                // Response rate
                ui.horizontal(|ui| {
                    ui.label("作答率:");
                    let pct = (app.response_rate * 100.0) as u32;
                    ui.monospace(
                        egui::RichText::new(format!("{pct}%"))
                            .strong()
                            .size(20.0),
                    );
                });

                // Response rate progress bar
                ui.add_space(4.0);
                let bar = egui::ProgressBar::new(app.response_rate)
                    .text(format!("{:.0}%", app.response_rate * 100.0));
                ui.add_sized(Vec2::new(170.0, 16.0), bar);

                ui.add_space(16.0);
                ui.separator();

                // Quiz panel
                ui.label(egui::RichText::new("测验面板").strong());
                ui.add_space(4.0);

                if app.student_count > 0 && app.response_rate < 1.0 {
                    ui.label("等待学生作答中…");
                    // Simulate gradual response
                    app.response_rate = (app.response_rate + 0.01).min(1.0);
                    ui.ctx().request_repaint();
                } else if app.response_rate >= 1.0 {
                    ui.label(
                        egui::RichText::new("所有学生已作答!")
                            .color(Color32::from_rgb(0x4C, 0xAF, 0x50)),
                    );
                } else {
                    ui.label("点击「快速测验」发起测验");
                }

                ui.add_space(8.0);
                if ui.button("结束测验").clicked() {
                    app.response_rate = 0.0;
                    app.student_count = 0;
                    app.set_status("测验已结束");
                }
            });
        });

    // ── Central canvas (fullscreen) ──
    egui::CentralPanel::default().show_inside(ui, |ui| {
        let (rect, _) = ui.allocate_at_least(ui.available_size(), egui::Sense::drag());

        // Canvas background — dark green for blackboard feel
        ui.painter()
            .rect_filled(rect, egui::Rounding::same(4.0), Color32::from_rgb(0x1A, 0x2E, 0x1A));

        // Render elements (simple text labels for MVP)
        let elements: Vec<_> = if app.selected_slide < app.slides.len() {
            app.slides[app.selected_slide].clone()
        } else {
            app.elements.clone()
        };

        if elements.is_empty() {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "上课模式 — 从备课视图添加内容",
                egui::FontId::proportional(18.0),
                Color32::from_gray(140),
            );
        } else {
            let mut y = rect.top() + 20.0;
            for elem in &elements {
                ui.painter().text(
                    egui::pos2(rect.left() + 20.0, y),
                    egui::Align2::LEFT_TOP,
                    format!("[{}] {}", elem.element_type(), elem.id()),
                    egui::FontId::proportional(14.0),
                    Color32::from_gray(200),
                );
                y += 22.0;
                if y > rect.bottom() - 20.0 {
                    break;
                }
            }
        }

        // Show annotation tool indicator
        let tool_name = match app.annotation_tool {
            AnnotationTool::Pen => "钢笔",
            AnnotationTool::Highlighter => "荧光笔",
            AnnotationTool::Eraser => "橡皮擦",
        };
        ui.painter().text(
            rect.right_bottom() + egui::vec2(-12.0, -12.0),
            egui::Align2::RIGHT_BOTTOM,
            format!("批注: {tool_name}"),
            egui::FontId::proportional(12.0),
            Color32::from_gray(160),
        );
    });
}
