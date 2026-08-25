//! GPU 渲染层
//!
//! 通过 egui Painter 提交曲线 Mesh，底层由 egui_wgpu 后端执行 GPU 光栅化。
//! 预分配屏幕坐标缓冲区，每帧仅清空填充，避免堆分配。

use crate::sampler::SampledSegments;
use crate::types::Viewport;
use crate::viewport::{nice_grid_interval, CoordTransform};
use egui::{Color32, FontId, Painter, Pos2, Rect, Shape, Stroke};

/// 渲染配置
pub struct RenderConfig {
    /// 背景色
    pub bg_color: Color32,
    /// 网格线颜色（次级）
    pub grid_color: Color32,
    /// 主网格线颜色（轴）
    pub axis_color: Color32,
    /// 坐标轴标签颜色
    pub label_color: Color32,
    /// 曲线线宽
    pub curve_width: f32,
    /// 网格线宽
    pub grid_width: f32,
    /// 轴线宽
    pub axis_width: f32,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            bg_color: Color32::from_rgb(24, 28, 38),
            grid_color: Color32::from_rgba_premultiplied(60, 65, 80, 60),
            axis_color: Color32::from_rgba_premultiplied(120, 130, 150, 180),
            label_color: Color32::from_rgb(160, 170, 190),
            curve_width: 2.0,
            grid_width: 0.5,
            axis_width: 1.5,
        }
    }
}

/// 曲线渲染器
///
/// 持有预分配的屏幕坐标缓冲区，避免每帧重新分配。
pub struct CurveRenderer {
    /// 预分配的屏幕坐标缓冲区（复用）
    screen_points: Vec<Pos2>,
    /// 渲染配置
    pub config: RenderConfig,
}

impl Default for CurveRenderer {
    fn default() -> Self {
        Self {
            screen_points: Vec::with_capacity(4096),
            config: RenderConfig::default(),
        }
    }
}

impl CurveRenderer {
    /// 绘制背景
    pub fn draw_background(&self, painter: &Painter, rect: Rect) {
        painter.rect_filled(rect, 0.0, self.config.bg_color);
    }

    /// 绘制网格和坐标轴
    pub fn draw_grid(
        &self,
        painter: &Painter,
        viewport: &Viewport,
        transform: &CoordTransform,
        rect: Rect,
    ) {
        let x_interval = nice_grid_interval(viewport.width(), 80.0, transform.scale_x);
        let y_interval = nice_grid_interval(viewport.height(), 60.0, transform.scale_y);

        let grid_stroke = Stroke::new(self.config.grid_width, self.config.grid_color);
        let axis_stroke = Stroke::new(self.config.axis_width, self.config.axis_color);

        // ── 垂直网格线 ──
        let x_start = (viewport.x_min / x_interval).floor() * x_interval;
        let mut x = x_start;
        while x <= viewport.x_max {
            let screen_pos = transform.world_to_screen(viewport, x, 0.0);
            let is_axis = x.abs() < x_interval * 0.01;
            let stroke = if is_axis { axis_stroke } else { grid_stroke };

            painter.line_segment(
                [
                    Pos2::new(screen_pos.x, rect.min.y),
                    Pos2::new(screen_pos.x, rect.max.y),
                ],
                stroke,
            );

            // X 轴标签
            {
                let label = format_number(x, x_interval);
                painter.text(
                    Pos2::new(screen_pos.x + 4.0, rect.max.y - 16.0),
                    egui::Align2::LEFT_TOP,
                    label,
                    FontId::proportional(11.0),
                    self.config.label_color,
                );
            }

            x += x_interval;
        }

        // ── 水平网格线 ──
        let y_start = (viewport.y_min / y_interval).floor() * y_interval;
        let mut y = y_start;
        while y <= viewport.y_max {
            let screen_pos = transform.world_to_screen(viewport, 0.0, y);
            let is_axis = y.abs() < y_interval * 0.01;
            let stroke = if is_axis { axis_stroke } else { grid_stroke };

            painter.line_segment(
                [
                    Pos2::new(rect.min.x, screen_pos.y),
                    Pos2::new(rect.max.x, screen_pos.y),
                ],
                stroke,
            );

            // Y 轴标签
            let label = format_number(y, y_interval);
            painter.text(
                Pos2::new(rect.min.x + 4.0, screen_pos.y - 8.0),
                egui::Align2::LEFT_TOP,
                label,
                FontId::proportional(11.0),
                self.config.label_color,
            );

            y += y_interval;
        }
    }

    /// 绘制一条函数曲线（可能包含多个段）
    ///
    /// 使用预分配缓冲区，避免每帧堆分配。通过 `Shape::line` 提交到 egui Painter，
    /// 底层由 egui_wgpu 在 GPU 端光栅化。
    pub fn draw_curve(
        &mut self,
        painter: &Painter,
        segments: &SampledSegments,
        color: Color32,
        viewport: &Viewport,
        transform: &CoordTransform,
    ) {
        let stroke = Stroke::new(self.config.curve_width, color);

        for segment in segments {
            if segment.len() < 2 {
                continue;
            }

            // 复用预分配缓冲区
            self.screen_points.clear();
            self.screen_points.reserve(segment.len());

            for point in segment {
                let sp = transform.world_to_screen(viewport, point[0], point[1]);
                self.screen_points.push(sp);
            }

            // 提交到 Painter → egui_wgpu GPU 光栅化
            painter.add(Shape::line(self.screen_points.clone(), stroke));
        }
    }

    /// 绘制函数标签（在曲线右端旁标注表达式）
    pub fn draw_curve_label(
        &self,
        painter: &Painter,
        label: &str,
        color: Color32,
        viewport: &Viewport,
        transform: &CoordTransform,
        segments: &SampledSegments,
    ) {
        // 找到最右侧的有效点
        let last_point = segments
            .iter()
            .flat_map(|s| s.last())
            .max_by(|a, b| a[0].partial_cmp(&b[0]).unwrap_or(std::cmp::Ordering::Equal));

        if let Some(pt) = last_point {
            let screen_pos = transform.world_to_screen(viewport, pt[0], pt[1]);
            painter.text(
                Pos2::new(screen_pos.x + 8.0, screen_pos.y - 8.0),
                egui::Align2::LEFT_CENTER,
                label,
                FontId::proportional(12.0),
                color,
            );
        }
    }
}

/// 格式化数字为简洁字符串
fn format_number(value: f64, interval: f64) -> String {
    if value.abs() < interval * 0.01 {
        return "0".to_string();
    }

    // 根据间隔决定小数位数
    let decimals = if interval >= 1.0 {
        0
    } else if interval >= 0.1 {
        1
    } else if interval >= 0.01 {
        2
    } else {
        3
    };

    // 大数字用科学计数法
    if value.abs() >= 10000.0 {
        format!("{value:.1e}")
    } else {
        format!("{value:.decimals$}")
    }
}
