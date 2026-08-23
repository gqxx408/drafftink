//! Grading view (批改).
//!
//! Layout:
//! ```text
//! ┌─────────────┬──────────────────────────────┬─────────────────┐
//! │ Submissions │     drftx File Viewer        │  Grading Panel  │
//! │  ┌───────┐  │                              │  Score: [____]  │
//! │  │ Stu 1 │  │   (element list / preview)   │  Comment:       │
//! │  │ Stu 2 │  │                              │  [_________]    │
//! │  │ Stu 3 │  │   ── red pen overlay ──      │  [Voice]        │
//! │  └───────┘  │                              │  [Submit Grade] │
//! └─────────────┴──────────────────────────────┴─────────────────┘
//! ```

use egui::Color32;

use crate::app::DesktopApp;

/// Mock submission data for the MVP.
struct MockSubmission {
    student_name: &'static str,
    homework_title: &'static str,
    submitted: bool,
    score: Option<f32>,
}

/// Static list of mock submissions.
const MOCK_SUBMISSIONS: &[MockSubmission] = &[
    MockSubmission {
        student_name: "张三",
        homework_title: "第一课 · 练习",
        submitted: true,
        score: None,
    },
    MockSubmission {
        student_name: "李四",
        homework_title: "第一课 · 练习",
        submitted: true,
        score: Some(85.0),
    },
    MockSubmission {
        student_name: "王五",
        homework_title: "第一课 · 练习",
        submitted: true,
        score: None,
    },
    MockSubmission {
        student_name: "赵六",
        homework_title: "第一课 · 练习",
        submitted: false,
        score: None,
    },
    MockSubmission {
        student_name: "钱七",
        homework_title: "第一课 · 练习",
        submitted: true,
        score: Some(92.0),
    },
];

/// Render the grade view inside the central panel.
pub fn show(app: &mut DesktopApp, ui: &mut egui::Ui) {
    // ── Left panel: submission list ──
    egui::SidePanel::left("grade_submissions")
        .resizable(true)
        .exact_width(200.0)
        .show_inside(ui, |ui| {
            ui.heading("学生提交");
            ui.separator();

            for (i, sub) in MOCK_SUBMISSIONS.iter().enumerate() {
                let is_selected = app.selected_submission == i;

                let status_icon = if !sub.submitted {
                    "○"
                } else if sub.score.is_some() {
                    "✓"
                } else {
                    "●"
                };

                let status_color = if !sub.submitted {
                    Color32::from_gray(120)
                } else if sub.score.is_some() {
                    Color32::from_rgb(0x4C, 0xAF, 0x50)
                } else {
                    Color32::from_rgb(0xFF, 0x98, 0x00)
                };

                let resp = ui.add_sized(
                    egui::Vec2::new(190.0, 56.0),
                    egui::SelectableLabel::new(is_selected, ""),
                );

                // Custom rendering inside the label area
                let rect = resp.rect;
                ui.painter().text(
                    rect.left_top() + egui::vec2(8.0, 6.0),
                    egui::Align2::LEFT_TOP,
                    status_icon,
                    egui::FontId::proportional(16.0),
                    status_color,
                );
                ui.painter().text(
                    rect.left_top() + egui::vec2(28.0, 6.0),
                    egui::Align2::LEFT_TOP,
                    sub.student_name,
                    egui::FontId::proportional(14.0),
                    if is_selected {
                        Color32::WHITE
                    } else {
                        Color32::from_gray(200)
                    },
                );
                ui.painter().text(
                    rect.left_top() + egui::vec2(28.0, 24.0),
                    egui::Align2::LEFT_TOP,
                    sub.homework_title,
                    egui::FontId::proportional(11.0),
                    Color32::from_gray(140),
                );

                if let Some(score) = sub.score {
                    ui.painter().text(
                        rect.right_top() + egui::vec2(-8.0, 6.0),
                        egui::Align2::RIGHT_TOP,
                        format!("{score:.0}"),
                        egui::FontId::proportional(16.0),
                        Color32::from_rgb(0x4C, 0xAF, 0x50),
                    );
                }

                if resp.clicked() {
                    app.selected_submission = i;
                    // Load score into the input if already graded
                    app.grade_score = sub.score.map(|s| s.to_string()).unwrap_or_default();
                    app.grade_comment.clear();
                    app.set_status(format!("查看: {} 的提交", sub.student_name));
                }
            }
        });

    // ── Right panel: grading ──
    egui::SidePanel::right("grade_panel")
        .resizable(true)
        .exact_width(240.0)
        .show_inside(ui, |ui| {
            ui.heading("批改");
            ui.separator();

            let sub = &MOCK_SUBMISSIONS[app.selected_submission.min(MOCK_SUBMISSIONS.len() - 1)];

            if !sub.submitted {
                ui.label(
                    egui::RichText::new("该学生尚未提交作业")
                        .color(Color32::from_rgb(0xF4, 0x43, 0x36)),
                );
                return;
            }

            ui.label(egui::RichText::new("学生:").small());
            ui.label(sub.student_name.to_string());
            ui.add_space(8.0);

            // Score input
            ui.label(egui::RichText::new("分数 (0-100):").small());
            ui.add(
                egui::TextEdit::singleline(&mut app.grade_score)
                    .hint_text("输入分数")
                    .desired_width(120.0),
            );
            ui.add_space(8.0);

            // Comment text area
            ui.label(egui::RichText::new("评语:").small());
            ui.add(
                egui::TextEdit::multiline(&mut app.grade_comment)
                    .hint_text("输入评语…")
                    .desired_width(220.0)
                    .desired_rows(4),
            );
            ui.add_space(8.0);

            // Voice comment button
            if ui.button("录音评语").clicked() {
                app.set_status("录音功能开发中…");
            }
            ui.add_space(8.0);
            ui.separator();

            // Submit grade button
            let submit = ui.add_sized(
                egui::Vec2::new(220.0, 36.0),
                egui::Button::new("提交成绩"),
            );
            if submit.clicked() {
                handle_submit_grade(app);
            }
        });

    // ── Central panel: drftx viewer ──
    egui::CentralPanel::default().show_inside(ui, |ui| {
        let (rect, _) = ui.allocate_at_least(ui.available_size(), egui::Sense::drag());

        let sub = &MOCK_SUBMISSIONS[app.selected_submission.min(MOCK_SUBMISSIONS.len() - 1)];

        // Viewer background
        ui.painter().rect_filled(
            rect,
            egui::Rounding::same(4.0),
            Color32::from_rgb(0x1E, 0x1E, 0x1E),
        );

        if !sub.submitted {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "该学生尚未提交作业",
                egui::FontId::proportional(18.0),
                Color32::from_gray(120),
            );
            return;
        }

        // Header
        ui.painter().text(
            rect.left_top() + egui::vec2(12.0, 12.0),
            egui::Align2::LEFT_TOP,
            format!("{} — {}", sub.student_name, sub.homework_title),
            egui::FontId::proportional(16.0),
            Color32::from_gray(200),
        );

        // Simulated drftx content preview
        ui.painter().text(
            rect.left_top() + egui::vec2(12.0, 44.0),
            egui::Align2::LEFT_TOP,
            "作业内容预览 (drftx viewer)",
            egui::FontId::proportional(13.0),
            Color32::from_gray(160),
        );

        // Red pen annotation overlay (simulated)
        if sub.score.is_none() && !app.grade_score.is_empty() {
            ui.painter().text(
                rect.right_top() + egui::vec2(-12.0, 12.0),
                egui::Align2::RIGHT_TOP,
                format!("红笔批注: {} 分", app.grade_score),
                egui::FontId::proportional(14.0),
                Color32::from_rgb(0xF4, 0x43, 0x36),
            );
        }

        // Simulated red pen strokes
        let stroke = egui::Stroke::new(2.0_f32, Color32::from_rgb(0xF4, 0x43, 0x36));
        let cx = rect.center().x;
        let cy = rect.center().y;
        ui.painter().line_segment(
            [
                egui::pos2(cx - 60.0, cy - 20.0),
                egui::pos2(cx + 60.0, cy - 20.0),
            ],
            stroke,
        );
        ui.painter().line_segment(
            [
                egui::pos2(cx - 40.0, cy),
                egui::pos2(cx + 40.0, cy),
            ],
            stroke,
        );
    });
}

/// Handle grade submission to the backend API.
fn handle_submit_grade(app: &mut DesktopApp) {
    let score: Result<f32, _> = app.grade_score.trim().parse();

    match score {
        Ok(s) if (0.0..=100.0).contains(&s) => {
            let sub = &MOCK_SUBMISSIONS[app.selected_submission.min(MOCK_SUBMISSIONS.len() - 1)];

            // In a real app, this would POST to the backend API.
            // For the MVP, we simulate the API call.
            log::info!(
                "[grade] Submitting grade: student={}, score={s}, comment={}",
                sub.student_name,
                app.grade_comment,
            );

            if app.jwt_token.is_none() {
                app.set_status("未登录，成绩已本地保存（请在设置中登录）");
            } else {
                app.set_status(format!(
                    "成绩已提交: {} = {:.0} 分",
                    sub.student_name, s
                ));
            }
        }
        Ok(_) => {
            app.set_status("分数必须在 0-100 之间");
        }
        Err(_) => {
            app.set_status("请输入有效的数字分数");
        }
    }
}
