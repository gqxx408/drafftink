//! 双屏学生视图
//!
//! 在第二屏幕（投影仪/大屏）上全屏显示：
//! - 题目内容（大字显示）
//! - 实时倒计时
//! - 抢答提示
//! - 答题结果（正确率、选项分布）
//!
//! # 双屏架构
//! ```text
//! 教师屏 (Screen 0)          学生屏 (Screen 1)
//! ┌─────────────────┐       ┌─────────────────┐
//! │  控制面板       │       │  题目大字显示    │
//! │  实时统计       │  →    │  倒计时动画      │
//! │  学生列表       │       │  答题结果展示    │
//! └─────────────────┘       └─────────────────┘
//! ```
//!
//! 逻辑代码完全复用，仅视图层不同。

use egui::{Color32, Frame, Pos2, Rect, RichText, Vec2};
use std::sync::{Arc, Mutex};

use crate::actors::ui::UiState;
use crate::messages::SessionSnapshot;
use crate::types::*;
use crate::ui::bar_chart::{self, BarChartConfig};

/// 学生屏渲染器
pub struct StudentScreen {
    /// 柱状图配置
    bar_config: BarChartConfig,
    /// 动画计时器
    animation_timer: f64,
    /// 上次事件计数（预留用于增量渲染优化）
    #[allow(dead_code)]
    last_event_count: u64,
}

impl Default for StudentScreen {
    fn default() -> Self {
        Self {
            bar_config: BarChartConfig {
                background: Color32::from_rgb(15, 20, 35),
                text_color: Color32::WHITE,
                ..Default::default()
            },
            animation_timer: 0.0,
            last_event_count: 0,
        }
    }
}

impl StudentScreen {
    /// 渲染学生屏视图
    ///
    /// 在第二屏幕的 egui 窗口中调用。
    /// 直接读取 `ui_state` 的最新快照。
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        ui_state: &Arc<Mutex<UiState>>,
    ) {
        let mut state = ui_state.lock().unwrap();
        let has_changed = state.has_new_events();
        let snapshot = state.snapshot().cloned();
        let last_quick_answer = state.last_quick_answer.clone();
        drop(state);

        // 动画
        if has_changed {
            self.animation_timer = 0.0;
        }
        self.animation_timer = (self.animation_timer + ctx.input(|i| i.unstable_dt) as f64 * 3.0).min(1.0);
        self.bar_config.animation_progress = self.animation_timer as f32;

        let snapshot = snapshot.unwrap_or_default();

        // 全屏暗色背景
        egui::CentralPanel::default()
            .frame(Frame::none().fill(Color32::from_rgb(15, 20, 35)))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                let painter = ui.painter_at(rect);

                // 背景粒子效果（简化版星空）
                Self::draw_background_particles(&painter, rect);

                match snapshot.status {
                    SessionStatus::Waiting => {
                        Self::render_waiting_screen(ui, rect);
                    }
                    SessionStatus::Active => {
                        self.render_active_screen(ui, rect, &snapshot, last_quick_answer);
                    }
                    SessionStatus::Paused => {
                        Self::render_paused_screen(ui, rect);
                    }
                    SessionStatus::Ended => {
                        Self::render_ended_screen(ui, rect, &snapshot);
                    }
                }
            });

        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }

    // ── 等待画面 ────────────────────────────────────────────────

    fn render_waiting_screen(ui: &mut egui::Ui, rect: Rect) {
        ui.put(
            Rect::from_min_size(rect.center() - Vec2::new(200.0, 40.0), Vec2::new(400.0, 80.0)),
            egui::Label::new(
                RichText::new("准备答题")
                    .size(48.0)
                    .color(Color32::from_rgb(200, 210, 230)),
            ),
        );

        ui.put(
            Rect::from_min_size(rect.center() - Vec2::new(200.0, -40.0), Vec2::new(400.0, 40.0)),
            egui::Label::new(
                RichText::new("请打开平板/手机，连接到本教室")
                    .size(20.0)
                    .color(Color32::from_rgb(140, 150, 170)),
            ),
        );
    }

    // ── 答题中画面 ──────────────────────────────────────────────

    fn render_active_screen(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        snapshot: &SessionSnapshot,
        last_quick_answer: Option<QuickAnswerResult>,
    ) {
        let question = match &snapshot.current_question {
            Some(q) => q,
            None => return,
        };

        let top_y = rect.min.y + 40.0;
        let mut current_y = top_y;

        // ── 题型标签 ──
        let type_label = match question.question_type {
            QuestionType::SingleChoice => "单选题",
            QuestionType::MultipleChoice => "多选题",
            QuestionType::TrueFalse => "判断题",
            QuestionType::QuickAnswer => "抢答",
            QuestionType::Text => "主观题",
        };

        ui.put(
            Rect::from_min_size(Pos2::new(rect.min.x + 40.0, current_y), Vec2::new(200.0, 30.0)),
            egui::Label::new(
                RichText::new(type_label)
                    .size(18.0)
                    .color(Color32::from_rgb(100, 180, 255)),
            ),
        );
        current_y += 40.0;

        // ── 题干（大字） ──
        let question_font = egui::FontId::proportional(36.0);
        let text_size = ui.fonts(|f| f.row_height(&question_font));
        let question_label = egui::Label::new(
            RichText::new(&question.content)
                .font(question_font)
                .strong()
                .color(Color32::WHITE),
        );
        ui.put(
            Rect::from_min_size(
                Pos2::new(rect.min.x + 40.0, current_y),
                Vec2::new(rect.width() - 80.0, text_size),
            ),
            question_label,
        );
        current_y += text_size + 30.0;

        // ── 选项（大字） ──
        if !question.options.is_empty() {
            for (i, opt) in question.options.iter().enumerate() {
                let label = format!("    {}.  {}", (b'A' + i as u8) as char, opt);
                ui.put(
                    Rect::from_min_size(
                        Pos2::new(rect.min.x + 60.0, current_y),
                        Vec2::new(rect.width() - 120.0, 40.0),
                    ),
                    egui::Label::new(
                        RichText::new(&label)
                            .size(24.0)
                            .color(Color32::from_rgb(220, 225, 240)),
                    ),
                );
                current_y += 40.0;
            }
        }

        current_y += 30.0;

        // ── 倒计时 ──
        if question.time_limit_sec > 0 {
            ui.put(
                Rect::from_min_size(
                    Pos2::new(rect.min.x + 40.0, current_y),
                    Vec2::new(200.0, 50.0),
                ),
                egui::Label::new(
                    RichText::new(format!("⏱ {}s", question.time_limit_sec))
                        .size(40.0)
                        .color(Color32::from_rgb(255, 200, 50)),
                ),
            );
            current_y += 60.0;
        }

        // ── 抢答胜者 ──
        if let Some(ref winner) = last_quick_answer {
            ui.put(
                Rect::from_min_size(
                    Pos2::new(rect.min.x + 40.0, current_y),
                    Vec2::new(rect.width() - 80.0, 60.0),
                ),
                egui::Label::new(
                    RichText::new(format!("🏆 {} 抢答成功！({}ms)", winner.winner_name, winner.response_time_ms))
                        .size(36.0)
                        .color(Color32::from_rgb(46, 204, 113))
                        .strong(),
                ),
            );
            current_y += 80.0;
        }

        // ── 实时统计标签 ──
        if let Some(ref stats) = snapshot.current_stats {
            ui.put(
                Rect::from_min_size(
                    Pos2::new(rect.min.x + 40.0, current_y),
                    Vec2::new(rect.width() - 80.0, 30.0),
                ),
                egui::Label::new(
                    RichText::new(format!(
                        "已答: {}/{}  |  正确率: {:.0}%",
                        stats.answered_count,
                        snapshot.total_students,
                        stats.accuracy * 100.0,
                    ))
                    .size(20.0)
                    .color(Color32::from_rgb(180, 190, 210)),
                ),
            );
            current_y += 40.0;

            // ── 柱状图 ──
            let chart_height = (rect.max.y - current_y - 40.0).min(300.0);
            if chart_height > 100.0 {
                let chart_rect = Rect::from_min_size(
                    Pos2::new(rect.min.x + 40.0, current_y),
                    Vec2::new(rect.width() - 80.0, chart_height),
                );
                bar_chart::draw_bar_chart(
                    ui,
                    chart_rect,
                    stats,
                    snapshot.total_students.max(1) as u32,
                    &self.bar_config,
                );
            }
        }
    }

    // ── 暂停画面 ────────────────────────────────────────────────

    fn render_paused_screen(ui: &mut egui::Ui, rect: Rect) {
        ui.put(
            Rect::from_min_size(rect.center() - Vec2::new(200.0, 40.0), Vec2::new(400.0, 80.0)),
            egui::Label::new(
                RichText::new("⏸ 已暂停")
                    .size(48.0)
                    .color(Color32::from_rgb(241, 196, 15)),
            ),
        );
    }

    // ── 结束画面 ────────────────────────────────────────────────

    fn render_ended_screen(
        ui: &mut egui::Ui,
        rect: Rect,
        snapshot: &SessionSnapshot,
    ) {
        ui.put(
            Rect::from_min_size(rect.center() - Vec2::new(200.0, 40.0), Vec2::new(400.0, 80.0)),
            egui::Label::new(
                RichText::new("答题结束")
                    .size(48.0)
                    .color(Color32::from_rgb(200, 210, 230)),
            ),
        );

        if let Some(ref stats) = snapshot.current_stats {
            ui.put(
                Rect::from_min_size(rect.center() - Vec2::new(200.0, -40.0), Vec2::new(400.0, 40.0)),
                egui::Label::new(
                    RichText::new(format!(
                        "正确率: {:.0}%  |  平均耗时: {:.0}ms",
                        stats.accuracy * 100.0,
                        stats.avg_response_time_ms,
                    ))
                    .size(20.0)
                    .color(Color32::from_rgb(180, 190, 210)),
                ),
            );
        }
    }

    // ── 背景粒子效果 ────────────────────────────────────────────

    fn draw_background_particles(painter: &egui::Painter, rect: Rect) {
        // 简单的伪随机星空粒子
        let mut seed = 42u32;
        let mut rand = || -> f32 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed as f32) / (u32::MAX as f32)
        };

        for _ in 0..30 {
            let x = rect.min.x + rand() * rect.width();
            let y = rect.min.y + rand() * rect.height();
            let brightness = (100.0 + rand() * 100.0) as u8;
            let size = if rand() > 0.85 { 2.0 } else { 1.0 };

            painter.circle_filled(
                Pos2::new(x, y),
                size,
                Color32::from_rgb(brightness, brightness, brightness + 30),
            );
        }
    }
}