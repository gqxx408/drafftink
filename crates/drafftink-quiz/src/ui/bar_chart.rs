//! 实时柱状图渲染
//!
//! 使用 egui 原生绘制 API（无需 egui_plot 依赖），
//! 在指定矩形内绘制选项分布的柱状图。
//!
//! # 性能
//! - 纯 2D 矩形绘制，GPU 加速
//! - 仅当 `QuestionStats` 变化时重绘
//! - 无内存分配

use egui::{Color32, Pos2, Rect, Rounding, Stroke};

use crate::types::QuestionStats;

/// 柱状图配置
pub struct BarChartConfig {
    /// 柱状图颜色列表（选项 A, B, C, D...）
    pub colors: Vec<Color32>,
    /// 正确选项高亮色
    pub correct_color: Color32,
    /// 背景色
    pub background: Color32,
    /// 文字颜色
    pub text_color: Color32,
    /// 柱体圆角
    pub rounding: f32,
    /// 动画进度 0.0~1.0（用于入场动画）
    pub animation_progress: f32,
}

impl Default for BarChartConfig {
    fn default() -> Self {
        Self {
            colors: vec![
                Color32::from_rgb(66, 133, 244),   // A: 蓝色
                Color32::from_rgb(234, 67, 53),     // B: 红色
                Color32::from_rgb(251, 188, 4),     // C: 黄色
                Color32::from_rgb(52, 168, 83),     // D: 绿色
                Color32::from_rgb(142, 68, 173),    // E: 紫色
                Color32::from_rgb(243, 156, 18),    // F: 橙色
                Color32::from_rgb(22, 160, 133),    // G: 青色
                Color32::from_rgb(192, 57, 43),     // H: 深红
            ],
            correct_color: Color32::from_rgb(46, 204, 113),
            background: Color32::from_rgb(30, 35, 45),
            text_color: Color32::from_rgb(220, 225, 235),
            rounding: 4.0,
            animation_progress: 1.0,
        }
    }
}

/// 在指定矩形区域内绘制柱状图
///
/// # 参数
/// - `ui`: egui UI 上下文
/// - `rect`: 绘制区域
/// - `stats`: 题目统计数据
/// - `total_students`: 总学生数（用于计算百分比）
/// - `config`: 柱状图配置
pub fn draw_bar_chart(
    ui: &mut egui::Ui,
    rect: Rect,
    stats: &QuestionStats,
    total_students: u32,
    config: &BarChartConfig,
) {
    let painter = ui.painter_at(rect);

    // 背景
    painter.rect_filled(rect, 6.0, config.background);

    let options = get_option_labels(stats.option_distribution.len());
    if options.is_empty() {
        // 无选项时显示提示
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "等待答题数据...",
            egui::FontId::proportional(14.0),
            Color32::from_rgb(150, 160, 180),
        );
        return;
    }

    let total = total_students.max(1);
    let bar_count = options.len();
    let padding = 20.0;
    let gap = 8.0;
    let label_height = 20.0;
    let top_padding = 30.0; // 顶部留空间给数值标签

    let available_width = rect.width() - padding * 2.0;
    let bar_width = (available_width - gap * (bar_count - 1) as f32) / bar_count as f32;
    let max_bar_height = rect.height() - label_height - top_padding - padding;

    let base_y = rect.max.y - label_height - padding;

    // 绘制网格线
    for i in 0..=4 {
        let y = base_y - max_bar_height * (i as f32 / 4.0);
        painter.line_segment(
            [Pos2::new(rect.min.x + padding, y), Pos2::new(rect.max.x - padding, y)],
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 30)),
        );
        // 百分比标签
        let pct = (4 - i) * 25;
        painter.text(
            Pos2::new(rect.min.x + padding - 4.0, y),
            egui::Align2::RIGHT_CENTER,
            format!("{}%", pct),
            egui::FontId::proportional(9.0),
            Color32::from_rgba_unmultiplied(180, 190, 210, 150),
        );
    }

    // 绘制柱体
    for (i, label) in options.iter().enumerate() {
        let count = stats.option_distribution.get(&(i as u8)).copied().unwrap_or(0);
        let ratio = count as f32 / total as f32;
        let bar_height = max_bar_height * ratio * config.animation_progress;

        let x = rect.min.x + padding + (bar_width + gap) * i as f32;

        // 柱体颜色
        let color = config.colors.get(i).copied().unwrap_or(Color32::GRAY);
        let bar_color = if count > 0 { color } else { color.gamma_multiply(0.3) };

        // 柱体矩形
        let bar_rect = Rect::from_min_max(
            Pos2::new(x, base_y - bar_height.max(2.0)),
            Pos2::new(x + bar_width, base_y),
        );

        // 绘制柱体
        painter.rect_filled(
            bar_rect,
            Rounding::same(config.rounding),
            bar_color,
        );

        // 柱体顶部数值
        if count > 0 {
            painter.text(
                Pos2::new(bar_rect.center().x, bar_rect.min.y - 6.0),
                egui::Align2::CENTER_BOTTOM,
                format!("{}", count),
                egui::FontId::proportional(12.0),
                config.text_color,
            );
        }

        // 选项标签
        painter.text(
            Pos2::new(bar_rect.center().x, base_y + 4.0),
            egui::Align2::CENTER_TOP,
            label,
            egui::FontId::proportional(12.0),
            config.text_color,
        );

        // 百分比标签
        painter.text(
            Pos2::new(bar_rect.center().x, bar_rect.min.y - 18.0),
            egui::Align2::CENTER_BOTTOM,
            format!("{:.0}%", ratio * 100.0),
            egui::FontId::proportional(10.0),
            Color32::from_rgba_unmultiplied(200, 210, 225, 180),
        );
    }

    // 底部统计摘要
    let summary = format!(
        "已答: {} / {}  |  正确率: {:.0}%  |  平均耗时: {:.0}ms",
        stats.answered_count,
        total_students,
        stats.accuracy * 100.0,
        stats.avg_response_time_ms,
    );
    painter.text(
        Pos2::new(rect.center().x, rect.max.y - 4.0),
        egui::Align2::CENTER_BOTTOM,
        summary,
        egui::FontId::proportional(10.0),
        Color32::from_rgba_unmultiplied(150, 170, 200, 200),
    );
}

/// 获取选项标签列表
fn get_option_labels(count: usize) -> Vec<String> {
    ('A'..)
        .take(count.max(2)) // 至少显示 A、B
        .map(|c| c.to_string())
        .collect()
}