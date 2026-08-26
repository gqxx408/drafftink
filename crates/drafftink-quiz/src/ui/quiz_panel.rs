//! 教师端 Quiz 主面板
//!
//! 整合所有 UI 元素：题目展示、实时统计、学生列表、抢答动画。
//!
//! # 设计原则
//! - 数据与视图分离：所有状态来自 `UiState`，不在此处修改业务逻辑
//! - 渲染缓存：仅当 `UiState.event_count` 变化时重新计算布局
//! - 低延迟：统计更新通过 mpsc 推送，egui 被动读取

use egui::{
    Align2, Button, Color32, Frame, Id, Layout, Pos2, Rect, RichText, Rounding, ScrollArea, Vec2,
};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use crate::actors::ui::UiState;
use crate::messages::{SessionCommand, SessionSnapshot};
use crate::types::*;
use crate::ui::bar_chart::{self, BarChartConfig};

/// 教师端 Quiz 主面板
pub struct QuizPanel {
    /// 上次渲染的事件计数（用于检测变化，预留）
    #[allow(dead_code)]
    last_event_count: u64,
    /// 柱状图动画计时器
    animation_timer: f64,
    /// 柱状图配置
    bar_config: BarChartConfig,
    /// 是否显示学生列表
    show_student_list: bool,
    /// 新题目草稿（题干）
    question_draft: String,
    /// 新题目选项草稿
    options_draft: Vec<String>,
    /// 新题目选项数量
    option_count: usize,
    /// 新题目类型
    question_type_draft: QuestionType,
    /// 新题目正确答案
    correct_answer_draft: String,
    /// 新题目时限
    time_limit_draft: u32,
    /// 是否显示题目编辑面板
    show_question_editor: bool,
}

impl Default for QuizPanel {
    fn default() -> Self {
        Self {
            last_event_count: 0,
            animation_timer: 0.0,
            bar_config: BarChartConfig::default(),
            show_student_list: true,
            question_draft: String::new(),
            options_draft: vec![String::new(), String::new(), String::new(), String::new()],
            option_count: 4,
            question_type_draft: QuestionType::SingleChoice,
            correct_answer_draft: String::new(),
            time_limit_draft: 30,
            show_question_editor: false,
        }
    }
}

impl QuizPanel {
    /// 渲染教师端 Quiz 主面板
    ///
    /// 可嵌入任意 egui 容器中（Window、Area、CentralPanel 等）。
    /// `session_tx` 用于发送教师操作命令（开始/结束题目等）。
    /// `ui_state` 是 UI Proxy 的共享状态。
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        ui_state: &Arc<Mutex<UiState>>,
        session_tx: &mpsc::Sender<SessionCommand>,
    ) {
        let mut state = ui_state.lock().unwrap();
        let has_changed = state.has_new_events();
        let snapshot = state.snapshot().cloned();
        let last_quick_answer = state.last_quick_answer.clone();
        let error_msg = state.take_error();
        drop(state);

        // 更新动画计时器
        if has_changed {
            self.animation_timer = 0.0;
        }
        self.animation_timer =
            (self.animation_timer + ui.input(|i| i.unstable_dt) as f64 * 3.0).min(1.0);
        self.bar_config.animation_progress = ease_out_cubic(self.animation_timer as f32);

        let snapshot = snapshot.unwrap_or_default();

        // ── 顶部状态栏 ──
        Frame::none()
            .fill(Color32::from_rgb(40, 44, 55))
            .inner_margin(egui::Margin::symmetric(12.0, 6.0))
            .show(ui, |ui| {
                self.render_top_bar(ui, &Some(snapshot.clone()), error_msg, session_tx);
            });

        ui.separator();

        // ── 主内容区（带学生列表的水平分割）──
        if self.show_student_list && snapshot.total_students > 0 {
            ui.columns(2, |cols| {
                // 左侧：主内容
                self.render_main_content(&mut cols[0], &snapshot, last_quick_answer, session_tx);
                // 右侧：学生列表
                self.render_student_list(&mut cols[1], &snapshot);
            });
        } else {
            self.render_main_content(ui, &snapshot, last_quick_answer, session_tx);
        }

        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }

    /// 渲染主内容区（根据状态切换）
    fn render_main_content(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &SessionSnapshot,
        last_quick_answer: Option<QuickAnswerResult>,
        session_tx: &mpsc::Sender<SessionCommand>,
    ) {
        match snapshot.status {
            SessionStatus::Waiting | SessionStatus::Paused => {
                self.render_idle_view(ui, snapshot, session_tx);
            }
            SessionStatus::Active => {
                self.render_active_view(ui, snapshot, last_quick_answer, session_tx);
            }
            SessionStatus::Ended => {
                self.render_ended_view(ui, snapshot);
            }
        }
    }

    // ── 顶部状态栏 ──────────────────────────────────────────────

    fn render_top_bar(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &Option<SessionSnapshot>,
        error_msg: Option<String>,
        session_tx: &mpsc::Sender<SessionCommand>,
    ) {
        ui.horizontal(|ui| {
            ui.heading("🦀 drafftink-quiz");

            ui.separator();

            if let Some(snap) = snapshot {
                // 状态指示器
                let (status_text, status_color) = match snap.status {
                    SessionStatus::Waiting => ("等待中", Color32::from_rgb(150, 160, 180)),
                    SessionStatus::Active => ("答题中", Color32::from_rgb(46, 204, 113)),
                    SessionStatus::Paused => ("已暂停", Color32::from_rgb(241, 196, 15)),
                    SessionStatus::Ended => ("已结束", Color32::from_rgb(231, 76, 60)),
                };
                ui.label(RichText::new("●").color(status_color).size(16.0));
                ui.label(RichText::new(status_text).color(status_color));

                ui.separator();

                // 在线人数
                ui.label(format!(
                    "在线: {}/{}",
                    snap.online_students, snap.total_students
                ));

                ui.separator();

                // 总答题数
                ui.label(format!("答题: {}", snap.total_answers));
            }

            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                // 学生列表开关
                ui.toggle_value(&mut self.show_student_list, "学生列表");

                // 暂停/恢复按钮
                if let Some(snap) = snapshot {
                    let is_paused = snap.status == SessionStatus::Paused;
                    let is_active = snap.status == SessionStatus::Active;
                    if is_active || is_paused {
                        let btn_text = if is_paused {
                            "▶ 恢复"
                        } else {
                            "⏸ 暂停"
                        };
                        if ui.add_sized([70.0, 24.0], Button::new(btn_text)).clicked() {
                            let tx = session_tx.clone();
                            let paused = !is_paused;
                            tokio::spawn(async move {
                                let _ = tx.send(SessionCommand::SetPause { paused }).await;
                            });
                        }
                    }
                }

                // 出题按钮
                if ui.add_sized([70.0, 24.0], Button::new("＋ 出题")).clicked() {
                    self.show_question_editor = true;
                }
            });
        });

        // 错误提示
        if let Some(err) = error_msg {
            ui.label(
                RichText::new(format!("⚠ {}", err))
                    .color(Color32::from_rgb(255, 100, 100))
                    .size(12.0),
            );
        }
    }

    // ── 等待视图 ────────────────────────────────────────────────

    fn render_idle_view(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &SessionSnapshot,
        session_tx: &mpsc::Sender<SessionCommand>,
    ) {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);

            ui.label(
                RichText::new("等待学生连接...")
                    .size(24.0)
                    .color(Color32::from_rgb(150, 160, 180)),
            );

            ui.add_space(16.0);

            ui.label(format!(
                "已连接学生: {} 人  |  等待答题: {}",
                snapshot.online_students,
                snapshot
                    .total_students
                    .saturating_sub(snapshot.total_answers),
            ));

            ui.add_space(32.0);

            // 出题按钮
            if ui
                .add_sized(
                    [200.0, 48.0],
                    Button::new(RichText::new("＋ 开始出题").size(18.0)),
                )
                .clicked()
            {
                self.show_question_editor = true;
            }
        });

        // 出题弹窗
        if self.show_question_editor {
            self.render_question_editor(ui.ctx(), session_tx);
        }
    }

    // ── 答题中视图 ──────────────────────────────────────────────

    fn render_active_view(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &SessionSnapshot,
        last_quick_answer: Option<QuickAnswerResult>,
        session_tx: &mpsc::Sender<SessionCommand>,
    ) {
        let question = match &snapshot.current_question {
            Some(q) => q,
            None => {
                ui.label("等待题目...");
                return;
            }
        };

        // ── 题目区域 ──
        Frame::none()
            .fill(Color32::from_rgb(35, 40, 52))
            .rounding(Rounding::same(8.0))
            .inner_margin(16.0)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());

                // 题型标签
                let type_label = match question.question_type {
                    QuestionType::SingleChoice => "单选题",
                    QuestionType::MultipleChoice => "多选题",
                    QuestionType::TrueFalse => "判断题",
                    QuestionType::QuickAnswer => "抢答题",
                    QuestionType::Text => "主观题",
                };
                ui.label(
                    RichText::new(type_label)
                        .size(12.0)
                        .color(Color32::from_rgb(100, 180, 255)),
                );

                ui.add_space(4.0);

                // 题干
                ui.label(RichText::new(&question.content).size(20.0).strong());

                ui.add_space(12.0);

                // 选项列表
                if !question.options.is_empty() {
                    for (i, opt) in question.options.iter().enumerate() {
                        let label = format!("{}. {}", (b'A' + i as u8) as char, opt);
                        let count = snapshot
                            .current_stats
                            .as_ref()
                            .and_then(|s| s.option_distribution.get(&(i as u8)))
                            .copied()
                            .unwrap_or(0);

                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(&label)
                                    .size(15.0)
                                    .color(Color32::from_rgb(220, 225, 240)),
                            );
                            if count > 0 {
                                ui.label(
                                    RichText::new(format!("({}人)", count))
                                        .size(12.0)
                                        .color(Color32::from_rgb(100, 180, 255)),
                                );
                            }
                        });
                    }
                }

                ui.add_space(12.0);

                // 结束本题按钮
                ui.horizontal(|ui| {
                    if ui
                        .add_sized(
                            [120.0, 32.0],
                            Button::new(RichText::new("结束本题").color(Color32::WHITE))
                                .fill(Color32::from_rgb(231, 76, 60)),
                        )
                        .clicked()
                    {
                        let tx = session_tx.clone();
                        let qid = question.id.clone();
                        tokio::spawn(async move {
                            let (reply_tx, _) = tokio::sync::oneshot::channel();
                            let _ = tx
                                .send(SessionCommand::EndQuestion {
                                    question_id: qid,
                                    reply: reply_tx,
                                })
                                .await;
                        });
                    }

                    // 倒计时
                    if question.time_limit_sec > 0 {
                        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("⏱ {}s", question.time_limit_sec))
                                    .size(16.0)
                                    .color(Color32::from_rgb(255, 200, 50)),
                            );
                        });
                    }
                });
            });

        ui.add_space(16.0);

        // ── 抢答获胜者动画 ──
        if let Some(ref winner) = last_quick_answer {
            Frame::none()
                .fill(Color32::from_rgb(40, 50, 30))
                .rounding(Rounding::same(8.0))
                .inner_margin(16.0)
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("🏆 {} 抢答成功！", winner.winner_name))
                            .size(28.0)
                            .color(Color32::from_rgb(46, 204, 113))
                            .strong(),
                    );
                    ui.label(format!("响应时间: {}ms", winner.response_time_ms));
                });

            ui.add_space(16.0);
        }

        // ── 柱状图 ──
        if let Some(ref stats) = snapshot.current_stats {
            let chart_rect = Rect::from_min_size(
                Pos2::new(ui.min_rect().min.x, ui.min_rect().min.y),
                Vec2::new(
                    ui.available_width().min(600.0),
                    ui.available_height().min(300.0),
                ),
            );

            let total = snapshot.total_students.max(1) as u32;
            bar_chart::draw_bar_chart(ui, chart_rect, stats, total, &self.bar_config);
        }
    }

    // ── 结束视图 ────────────────────────────────────────────────

    fn render_ended_view(&mut self, ui: &mut egui::Ui, snapshot: &SessionSnapshot) {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);

            ui.label(
                RichText::new("答题已结束")
                    .size(28.0)
                    .color(Color32::from_rgb(200, 210, 230)),
            );

            ui.add_space(16.0);

            ui.label(format!(
                "共 {} 名学生参与，累计 {} 次答题",
                snapshot.total_students, snapshot.total_answers,
            ));

            ui.add_space(32.0);

            if let Some(ref stats) = snapshot.current_stats {
                ui.label(format!(
                    "正确率: {:.0}%  |  平均响应时间: {:.0}ms",
                    stats.accuracy * 100.0,
                    stats.avg_response_time_ms,
                ));
            }
        });
    }

    // ── 学生列表 ────────────────────────────────────────────────

    fn render_student_list(&mut self, ui: &mut egui::Ui, snapshot: &SessionSnapshot) {
        ui.heading("学生列表");
        ui.separator();

        ScrollArea::vertical().show(ui, |ui| {
            for student in &snapshot.students {
                ui.horizontal(|ui| {
                    // 在线状态指示器
                    let dot_color = if student.connected {
                        Color32::from_rgb(46, 204, 113)
                    } else {
                        Color32::from_rgb(150, 150, 150)
                    };
                    ui.label(RichText::new("●").color(dot_color).size(10.0));

                    ui.label(&student.name);

                    if let Some(ref device) = student.device_id {
                        ui.label(
                            RichText::new(format!("[{}]", device))
                                .size(10.0)
                                .color(Color32::from_rgb(120, 130, 150)),
                        );
                    }
                });
            }
        });
    }

    // ── 出题编辑器 ──────────────────────────────────────────────

    fn render_question_editor(
        &mut self,
        ctx: &egui::Context,
        session_tx: &mpsc::Sender<SessionCommand>,
    ) {
        egui::Window::new("出题")
            .id(Id::new("quiz_question_editor"))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                // 题型选择
                ui.horizontal(|ui| {
                    ui.label("题型:");
                    ui.selectable_value(
                        &mut self.question_type_draft,
                        QuestionType::SingleChoice,
                        "单选",
                    );
                    ui.selectable_value(
                        &mut self.question_type_draft,
                        QuestionType::MultipleChoice,
                        "多选",
                    );
                    ui.selectable_value(
                        &mut self.question_type_draft,
                        QuestionType::TrueFalse,
                        "判断",
                    );
                    ui.selectable_value(
                        &mut self.question_type_draft,
                        QuestionType::QuickAnswer,
                        "抢答",
                    );
                });

                ui.separator();

                // 题干
                ui.label("题干:");
                ui.text_edit_multiline(&mut self.question_draft);

                ui.separator();

                // 选项（仅选择题）
                if matches!(
                    self.question_type_draft,
                    QuestionType::SingleChoice | QuestionType::MultipleChoice
                ) {
                    ui.label("选项:");
                    ui.horizontal(|ui| {
                        if ui.button("＋").clicked() && self.option_count < 8 {
                            self.option_count += 1;
                            self.options_draft.push(String::new());
                        }
                        if ui.button("－").clicked() && self.option_count > 2 {
                            self.option_count -= 1;
                            self.options_draft.pop();
                        }
                    });

                    for i in 0..self.option_count {
                        let label = format!("{}:", (b'A' + i as u8) as char);
                        ui.horizontal(|ui| {
                            ui.label(&label);
                            ui.text_edit_singleline(&mut self.options_draft[i]);
                        });
                    }
                }

                ui.separator();

                // 正确答案
                if !matches!(self.question_type_draft, QuestionType::Text) {
                    ui.label("正确答案:");
                    ui.text_edit_singleline(&mut self.correct_answer_draft);
                }

                // 时限
                ui.horizontal(|ui| {
                    ui.label("答题时限(秒):");
                    ui.add(egui::DragValue::new(&mut self.time_limit_draft).range(0..=600));
                    if self.time_limit_draft == 0 {
                        ui.label("(不限时)");
                    }
                });

                ui.separator();

                // 按钮
                ui.horizontal(|ui| {
                    if ui.button("取消").clicked() {
                        self.show_question_editor = false;
                    }

                    if ui
                        .add_sized([100.0, 28.0], Button::new("开始答题"))
                        .clicked()
                    {
                        let question = self.build_question();
                        let tx = session_tx.clone();
                        tokio::spawn(async move {
                            let (reply_tx, _) = tokio::sync::oneshot::channel();
                            let _ = tx
                                .send(SessionCommand::StartQuestion {
                                    question,
                                    reply: reply_tx,
                                })
                                .await;
                        });
                        self.show_question_editor = false;
                    }
                });
            });
    }

    /// 从编辑器草稿构建 Question
    fn build_question(&self) -> Question {
        let correct_answer = match self.question_type_draft {
            QuestionType::SingleChoice => self
                .correct_answer_draft
                .chars()
                .next()
                .and_then(|c| {
                    if c.is_ascii_uppercase() {
                        Some(c as u8 - b'A')
                    } else if c.is_ascii_digit() {
                        Some(c as u8 - b'0')
                    } else {
                        None
                    }
                })
                .map(CorrectAnswer::Single),
            QuestionType::MultipleChoice => {
                let indices: Vec<u8> = self
                    .correct_answer_draft
                    .chars()
                    .filter_map(|c| {
                        if c.is_ascii_uppercase() {
                            Some(c as u8 - b'A')
                        } else if c.is_ascii_digit() {
                            Some(c as u8 - b'0')
                        } else {
                            None
                        }
                    })
                    .collect();
                if indices.is_empty() {
                    None
                } else {
                    Some(CorrectAnswer::Multiple(indices))
                }
            }
            QuestionType::TrueFalse => self
                .correct_answer_draft
                .to_lowercase()
                .starts_with('t')
                .then_some(CorrectAnswer::Bool(true))
                .or_else(|| {
                    self.correct_answer_draft
                        .to_lowercase()
                        .starts_with('f')
                        .then_some(CorrectAnswer::Bool(false))
                }),
            QuestionType::Text => {
                if self.correct_answer_draft.is_empty() {
                    None
                } else {
                    Some(CorrectAnswer::Text(self.correct_answer_draft.clone()))
                }
            }
            QuestionType::QuickAnswer => None,
        };

        Question {
            id: uuid::Uuid::new_v4().to_string(),
            question_type: self.question_type_draft,
            content: self.question_draft.clone(),
            options: self.options_draft[..self.option_count].to_vec(),
            correct_answer,
            score: 10,
            time_limit_sec: self.time_limit_draft,
        }
    }
}

/// ease-out 缓动函数
fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}
