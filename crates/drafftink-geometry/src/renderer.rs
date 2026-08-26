//! 渲染层 — egui Painter GPU 加速渲染
//!
//! 通过 egui Painter 提交三角化后的 Mesh，底层由 egui_wgpu 后端执行 GPU 光栅化。
//! 所有绘制经过 GPU，禁止 CPU 软渲染。
//!
//! # 渲染流程
//! 1. solver.solve() → SolverContext（具体坐标）
//! 2. mesh::triangulate_*() → GeometryMesh（顶点 + 索引）
//! 3. GeometryMesh::add_to_painter() → egui::Shape::mesh → GPU
//!
//! 坐标变换：世界坐标 ↔ 屏幕坐标，通过 viewport_offset 和 zoom 控制。

use egui::{Color32, FontId, Painter, Pos2, Rect, Stroke, Vec2};
use uuid::Uuid;

use crate::definitions::Point2D;
use crate::mesh::{
    triangulate_annulus, triangulate_arc, triangulate_circle, triangulate_circle_stroke,
    triangulate_ellipse_stroke, triangulate_line, triangulate_polygon_fill,
    triangulate_polygon_stroke, triangulate_sector,
};
use crate::solver::SolverContext;

/// 渲染配置
pub struct RenderConfig {
    /// 背景色
    pub bg_color: Color32,
    /// 网格线颜色
    pub grid_color: Color32,
    /// 主轴颜色
    pub axis_color: Color32,
    /// 点颜色
    pub point_color: Color32,
    /// 选中点颜色
    pub point_selected_color: Color32,
    /// 悬停点颜色
    pub point_hover_color: Color32,
    /// 线段颜色
    pub line_color: Color32,
    /// 圆颜色
    pub circle_color: Color32,
    /// 标签颜色
    pub label_color: Color32,
    /// 点大小（半径）
    pub point_size: f32,
    /// 线宽
    pub line_width: f32,
    /// 圆描边宽度
    pub circle_stroke_width: f32,
    /// 多边形颜色
    pub polygon_color: Color32,
    /// 圆弧颜色
    pub arc_color: Color32,
    /// 扇形颜色
    pub sector_color: Color32,
    /// 椭圆颜色
    pub ellipse_color: Color32,
    /// 圆环颜色
    pub annulus_color: Color32,
    /// 贝塞尔曲线颜色
    pub bezier_color: Color32,
    /// 标注颜色
    pub annotation_color: Color32,
    /// 多边形描边宽度
    pub polygon_stroke_width: f32,
    /// 圆弧描边宽度
    pub arc_stroke_width: f32,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            bg_color: Color32::from_rgb(24, 28, 38),
            grid_color: Color32::from_rgba_premultiplied(60, 65, 80, 40),
            axis_color: Color32::from_rgba_premultiplied(120, 130, 150, 120),
            point_color: Color32::from_rgb(100, 180, 255),
            point_selected_color: Color32::from_rgb(255, 200, 80),
            point_hover_color: Color32::from_rgb(255, 150, 100),
            line_color: Color32::from_rgb(220, 225, 240),
            circle_color: Color32::from_rgb(100, 220, 120),
            label_color: Color32::from_rgb(160, 170, 190),
            point_size: 6.0,
            line_width: 2.5,
            circle_stroke_width: 2.0,
            polygon_color: Color32::from_rgb(180, 220, 255),
            arc_color: Color32::from_rgb(255, 200, 100),
            sector_color: Color32::from_rgba_premultiplied(100, 220, 120, 100),
            ellipse_color: Color32::from_rgb(220, 180, 255),
            annulus_color: Color32::from_rgb(255, 180, 180),
            bezier_color: Color32::from_rgb(180, 255, 200),
            annotation_color: Color32::from_rgb(255, 220, 100),
            polygon_stroke_width: 2.0,
            arc_stroke_width: 2.0,
        }
    }
}

/// 视口 — 世界坐标偏移和缩放
#[derive(Debug, Clone)]
pub struct Viewport {
    /// 视口偏移（屏幕坐标）
    pub offset: Vec2,
    /// 缩放系数
    pub zoom: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            offset: Vec2::ZERO,
            zoom: 1.0,
        }
    }
}

impl Viewport {
    /// 世界坐标 → 屏幕坐标
    pub fn world_to_screen(&self, world: Point2D, canvas_center: Vec2) -> Pos2 {
        Pos2::new(
            canvas_center.x + (world.x * self.zoom) + self.offset.x,
            canvas_center.y + (-world.y * self.zoom) + self.offset.y, // Y 轴翻转
        )
    }

    /// 屏幕坐标 → 世界坐标
    pub fn screen_to_world(&self, screen: Pos2, canvas_center: Vec2) -> Point2D {
        Point2D::new(
            (screen.x - canvas_center.x - self.offset.x) / self.zoom,
            -(screen.y - canvas_center.y - self.offset.y) / self.zoom,
        )
    }
}

/// 几何渲染器
///
/// 持有渲染配置和预分配的网格缓冲区。
/// 通过 egui Painter 提交 GPU 渲染。
#[derive(Default)]
pub struct GeometryRenderer {
    /// 渲染配置
    pub config: RenderConfig,
    /// 视口
    pub viewport: Viewport,
}

impl GeometryRenderer {
    /// 创建新渲染器
    pub fn new() -> Self {
        Self::default()
    }

    /// 绘制背景
    pub fn draw_background(&self, painter: &Painter, rect: Rect) {
        painter.rect_filled(rect, 0.0, self.config.bg_color);
    }

    /// 绘制网格
    pub fn draw_grid(&self, painter: &Painter, rect: Rect) {
        let center = rect.center();
        let canvas_center = Vec2::new(center.x, center.y);
        let zoom = self.viewport.zoom;

        // 网格间距（世界坐标）
        let grid_spacing = 50.0_f32; // 每 50 世界单位一条线
        let screen_spacing = grid_spacing * zoom;

        // 自适应：当缩放太小/太大时调整间距
        let (spacing, major_every) = if screen_spacing < 20.0 {
            (grid_spacing * 5.0, 2)
        } else if screen_spacing > 200.0 {
            (grid_spacing * 0.2, 5)
        } else {
            (grid_spacing, 5)
        };

        let screen_sp = spacing * zoom;
        let grid_stroke = Stroke::new(0.5_f32, self.config.grid_color);
        let axis_stroke = Stroke::new(1.5_f32, self.config.axis_color);

        // 垂直网格线
        let mut x = -(self.viewport.offset.x % screen_sp);
        let mut count = 0;
        while x < rect.width() {
            let screen_x = rect.min.x + x;
            let is_axis = count % major_every == 0;
            let stroke = if is_axis { axis_stroke } else { grid_stroke };
            painter.line_segment(
                [
                    Pos2::new(screen_x, rect.min.y),
                    Pos2::new(screen_x, rect.max.y),
                ],
                stroke,
            );
            x += screen_sp;
            count += 1;
        }

        // 水平网格线
        let mut y = -(self.viewport.offset.y % screen_sp);
        let mut count = 0;
        while y < rect.height() {
            let screen_y = rect.min.y + y;
            let is_axis = count % major_every == 0;
            let stroke = if is_axis { axis_stroke } else { grid_stroke };
            painter.line_segment(
                [
                    Pos2::new(rect.min.x, screen_y),
                    Pos2::new(rect.max.x, screen_y),
                ],
                stroke,
            );
            y += screen_sp;
            count += 1;
        }

        // 坐标轴标签（原点）
        let origin = self
            .viewport
            .world_to_screen(Point2D::new(0.0, 0.0), canvas_center);
        if rect.contains(origin) {
            painter.text(
                origin,
                egui::Align2::RIGHT_BOTTOM,
                "O",
                FontId::proportional(11.0),
                self.config.label_color,
            );
        }
    }

    /// 绘制点
    ///
    /// 使用 lyon 三角化生成填充圆，通过 egui Painter 提交到 GPU。
    pub fn draw_point(
        &self,
        painter: &Painter,
        world_pos: Point2D,
        canvas_center: Vec2,
        color: Color32,
    ) {
        let screen_pos = self.viewport.world_to_screen(world_pos, canvas_center);
        let screen_radius = self.config.point_size * self.viewport.zoom.min(2.0);

        // 使用 lyon 三角化 → GPU 渲染
        let mesh = triangulate_circle(Point2D::new(screen_pos.x, screen_pos.y), screen_radius);
        mesh.add_to_painter(painter, color);

        // 外圈高光
        painter.circle_stroke(
            screen_pos,
            screen_radius + 1.0,
            Stroke::new(
                1.5_f32,
                Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), 120),
            ),
        );
    }

    /// 绘制选中状态的点（带高亮环）
    pub fn draw_point_selected(&self, painter: &Painter, world_pos: Point2D, canvas_center: Vec2) {
        let screen_pos = self.viewport.world_to_screen(world_pos, canvas_center);
        let screen_radius = self.config.point_size * self.viewport.zoom.min(2.0);

        // 外环
        painter.circle_stroke(
            screen_pos,
            screen_radius + 5.0,
            Stroke::new(2.0_f32, self.config.point_selected_color),
        );

        // 内填充
        let mesh = triangulate_circle(Point2D::new(screen_pos.x, screen_pos.y), screen_radius);
        mesh.add_to_painter(painter, self.config.point_selected_color);
    }

    /// 绘制悬停状态的点
    pub fn draw_point_hover(&self, painter: &Painter, world_pos: Point2D, canvas_center: Vec2) {
        let screen_pos = self.viewport.world_to_screen(world_pos, canvas_center);
        let screen_radius = self.config.point_size * self.viewport.zoom.min(2.0);

        let mesh = triangulate_circle(
            Point2D::new(screen_pos.x, screen_pos.y),
            screen_radius + 2.0,
        );
        mesh.add_to_painter(painter, self.config.point_hover_color);
    }

    /// 绘制线段
    ///
    /// 使用 lyon StrokeTessellator 三角化粗线条 → GPU 渲染。
    pub fn draw_line(
        &self,
        painter: &Painter,
        start: Point2D,
        end: Point2D,
        canvas_center: Vec2,
        color: Color32,
    ) {
        let screen_start = self.viewport.world_to_screen(start, canvas_center);
        let screen_end = self.viewport.world_to_screen(end, canvas_center);

        let width = self.config.line_width * self.viewport.zoom.min(2.0);

        // lyon 三角化 → GPU
        let mesh = triangulate_line(
            Point2D::new(screen_start.x, screen_start.y),
            Point2D::new(screen_end.x, screen_end.y),
            width,
        );
        mesh.add_to_painter(painter, color);
    }

    /// 绘制圆
    ///
    /// 使用 lyon StrokeTessellator 三角化圆环 → GPU 渲染。
    pub fn draw_circle(
        &self,
        painter: &Painter,
        center: Point2D,
        radius: f32,
        canvas_center: Vec2,
        color: Color32,
    ) {
        let screen_center = self.viewport.world_to_screen(center, canvas_center);
        let screen_radius = radius * self.viewport.zoom;

        let width = self.config.circle_stroke_width * self.viewport.zoom.min(2.0);

        // lyon 三角化圆环描边 → GPU
        let mesh = triangulate_circle_stroke(
            Point2D::new(screen_center.x, screen_center.y),
            screen_radius,
            width,
        );
        mesh.add_to_painter(painter, color);
    }

    /// 绘制多边形（描边）
    pub fn draw_polygon(
        &self,
        painter: &Painter,
        points: &[Point2D],
        canvas_center: Vec2,
        color: Color32,
    ) {
        if points.len() < 2 {
            return;
        }
        let screen_pts: Vec<Point2D> = points
            .iter()
            .map(|p| {
                let s = self.viewport.world_to_screen(*p, canvas_center);
                Point2D::new(s.x, s.y)
            })
            .collect();
        let width = self.config.polygon_stroke_width * self.viewport.zoom.min(2.0);
        let mesh = triangulate_polygon_stroke(&screen_pts, width);
        mesh.add_to_painter(painter, color);
    }

    /// 绘制多边形（填充）
    pub fn draw_polygon_filled(
        &self,
        painter: &Painter,
        points: &[Point2D],
        canvas_center: Vec2,
        color: Color32,
    ) {
        if points.len() < 3 {
            return;
        }
        let screen_pts: Vec<Point2D> = points
            .iter()
            .map(|p| {
                let s = self.viewport.world_to_screen(*p, canvas_center);
                Point2D::new(s.x, s.y)
            })
            .collect();
        let mesh = triangulate_polygon_fill(&screen_pts);
        mesh.add_to_painter(painter, color);
    }

    /// 绘制正多边形（描边）
    #[allow(clippy::too_many_arguments)]
    pub fn draw_regular_polygon(
        &self,
        painter: &Painter,
        center: Point2D,
        radius: f32,
        sides: u32,
        rotation: f32,
        canvas_center: Vec2,
        color: Color32,
    ) {
        let screen_center = self.viewport.world_to_screen(center, canvas_center);
        let screen_radius = radius * self.viewport.zoom;
        let mut pts = Vec::with_capacity(sides as usize);
        for i in 0..sides {
            let angle = rotation + i as f32 / sides as f32 * std::f32::consts::TAU;
            pts.push(Point2D::new(
                screen_center.x + screen_radius * angle.cos(),
                screen_center.y + screen_radius * angle.sin(),
            ));
        }
        let width = self.config.polygon_stroke_width * self.viewport.zoom.min(2.0);
        let mesh = triangulate_polygon_stroke(&pts, width);
        mesh.add_to_painter(painter, color);
    }

    /// 绘制圆弧
    #[allow(clippy::too_many_arguments)]
    pub fn draw_arc(
        &self,
        painter: &Painter,
        center: Point2D,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
        canvas_center: Vec2,
        color: Color32,
    ) {
        let screen_center = self.viewport.world_to_screen(center, canvas_center);
        let screen_radius = radius * self.viewport.zoom;
        let width = self.config.arc_stroke_width * self.viewport.zoom.min(2.0);
        let mesh = triangulate_arc(
            Point2D::new(screen_center.x, screen_center.y),
            screen_radius,
            start_angle,
            end_angle,
            width,
        );
        mesh.add_to_painter(painter, color);
    }

    /// 绘制扇形（填充）
    #[allow(clippy::too_many_arguments)]
    pub fn draw_sector(
        &self,
        painter: &Painter,
        center: Point2D,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
        canvas_center: Vec2,
        color: Color32,
    ) {
        let screen_center = self.viewport.world_to_screen(center, canvas_center);
        let screen_radius = radius * self.viewport.zoom;
        let mesh = triangulate_sector(
            Point2D::new(screen_center.x, screen_center.y),
            screen_radius,
            start_angle,
            end_angle,
        );
        mesh.add_to_painter(painter, color);
    }

    /// 绘制椭圆（描边）
    #[allow(clippy::too_many_arguments)]
    pub fn draw_ellipse(
        &self,
        painter: &Painter,
        center: Point2D,
        semi_a: f32,
        semi_b: f32,
        rotation: f32,
        canvas_center: Vec2,
        color: Color32,
    ) {
        let screen_center = self.viewport.world_to_screen(center, canvas_center);
        let screen_a = semi_a * self.viewport.zoom;
        let screen_b = semi_b * self.viewport.zoom;
        let width = self.config.arc_stroke_width * self.viewport.zoom.min(2.0);
        let mesh = triangulate_ellipse_stroke(
            Point2D::new(screen_center.x, screen_center.y),
            screen_a,
            screen_b,
            rotation,
            width,
        );
        mesh.add_to_painter(painter, color);
    }

    /// 绘制圆环（填充）
    pub fn draw_annulus(
        &self,
        painter: &Painter,
        center: Point2D,
        inner_radius: f32,
        outer_radius: f32,
        canvas_center: Vec2,
        color: Color32,
    ) {
        let screen_center = self.viewport.world_to_screen(center, canvas_center);
        let screen_inner = inner_radius * self.viewport.zoom;
        let screen_outer = outer_radius * self.viewport.zoom;
        let mesh = triangulate_annulus(
            Point2D::new(screen_center.x, screen_center.y),
            screen_inner,
            screen_outer,
        );
        mesh.add_to_painter(painter, color);
    }

    /// 绘制贝塞尔曲线
    pub fn draw_bezier(
        &self,
        painter: &Painter,
        control_points: &[Point2D],
        canvas_center: Vec2,
        color: Color32,
    ) {
        if control_points.is_empty() {
            return;
        }
        let screen_pts: Vec<Point2D> = control_points
            .iter()
            .map(|p| {
                let s = self.viewport.world_to_screen(*p, canvas_center);
                Point2D::new(s.x, s.y)
            })
            .collect();

        let segments = 64;
        let width = self.config.line_width * self.viewport.zoom.min(2.0);

        match screen_pts.len() {
            2 => {
                // 线性
                let mesh = triangulate_line(screen_pts[0], screen_pts[1], width);
                mesh.add_to_painter(painter, color);
            }
            3 => {
                // 二阶贝塞尔
                let mut prev = screen_pts[0];
                for i in 1..=segments {
                    let t = i as f32 / segments as f32;
                    let p = quadratic_bezier(screen_pts[0], screen_pts[1], screen_pts[2], t);
                    let mesh = triangulate_line(prev, p, width);
                    mesh.add_to_painter(painter, color);
                    prev = p;
                }
            }
            4 => {
                // 三阶贝塞尔
                let mut prev = screen_pts[0];
                for i in 1..=segments {
                    let t = i as f32 / segments as f32;
                    let p = cubic_bezier(
                        screen_pts[0],
                        screen_pts[1],
                        screen_pts[2],
                        screen_pts[3],
                        t,
                    );
                    let mesh = triangulate_line(prev, p, width);
                    mesh.add_to_painter(painter, color);
                    prev = p;
                }
            }
            _ => {
                // 降级为线段连接
                for w in screen_pts.windows(2) {
                    let mesh = triangulate_line(w[0], w[1], width);
                    mesh.add_to_painter(painter, color);
                }
            }
        }
    }

    /// 绘制角度标注
    pub fn draw_angle_mark(
        &self,
        painter: &Painter,
        vertex: Point2D,
        point_a: Point2D,
        point_b: Point2D,
        canvas_center: Vec2,
        color: Color32,
    ) {
        let sv = self.viewport.world_to_screen(vertex, canvas_center);
        let sa = self.viewport.world_to_screen(point_a, canvas_center);
        let sb = self.viewport.world_to_screen(point_b, canvas_center);

        let angle_a = (sa - sv).angle();
        let angle_b = (sb - sv).angle();
        let radius = 30.0_f32
            .min((sa - sv).length() * 0.4)
            .min((sb - sv).length() * 0.4);

        // 绘制角度弧
        let mesh = triangulate_arc(Point2D::new(sv.x, sv.y), radius, angle_a, angle_b, 1.5);
        mesh.add_to_painter(painter, color);

        // 绘制角度值文本
        let mut angle_deg = (angle_b - angle_a).to_degrees();
        if angle_deg < 0.0 {
            angle_deg += 360.0;
        }
        if angle_deg > 180.0 {
            angle_deg = 360.0 - angle_deg;
        }
        let mid_angle = (angle_a + angle_b) * 0.5;
        let label_pos = Pos2::new(
            sv.x + (radius + 15.0) * mid_angle.cos(),
            sv.y + (radius + 15.0) * mid_angle.sin(),
        );
        painter.text(
            label_pos,
            egui::Align2::CENTER_CENTER,
            format!("{angle_deg:.0}°"),
            FontId::proportional(12.0),
            color,
        );
    }

    /// 绘制长度标注
    pub fn draw_length_mark(
        &self,
        painter: &Painter,
        start: Point2D,
        end: Point2D,
        canvas_center: Vec2,
        color: Color32,
    ) {
        let ss = self.viewport.world_to_screen(start, canvas_center);
        let se = self.viewport.world_to_screen(end, canvas_center);

        // 主线
        painter.line_segment([ss, se], Stroke::new(1.5_f32, color));

        // 端点小竖线
        let dir = (se - ss).normalized();
        let perp = Vec2::new(-dir.y, dir.x);
        let tick = 5.0;
        painter.line_segment(
            [ss + perp * tick, ss - perp * tick],
            Stroke::new(1.5_f32, color),
        );
        painter.line_segment(
            [se + perp * tick, se - perp * tick],
            Stroke::new(1.5_f32, color),
        );

        // 距离文本
        let dist = (end - start).norm();
        let mid = Pos2::new((ss.x + se.x) * 0.5, (ss.y + se.y) * 0.5);
        painter.text(
            mid,
            egui::Align2::CENTER_BOTTOM,
            format!("{dist:.1}"),
            FontId::proportional(12.0),
            color,
        );
    }

    /// 绘制坐标系网格（基于 GridDef 定义）
    #[allow(clippy::too_many_arguments)]
    pub fn draw_coordinate_grid(
        &self,
        painter: &Painter,
        origin: Point2D,
        spacing: f32,
        show_major: bool,
        major_every: u32,
        show_labels: bool,
        canvas_center: Vec2,
    ) {
        let screen_origin = self.viewport.world_to_screen(origin, canvas_center);
        let screen_spacing = spacing * self.viewport.zoom;
        if screen_spacing < 5.0 {
            return;
        }

        let clip_rect = painter.clip_rect();
        let grid_stroke = Stroke::new(0.5_f32, self.config.grid_color);
        let major_stroke = Stroke::new(1.0_f32, self.config.axis_color);
        let axis_stroke = Stroke::new(2.0_f32, self.config.axis_color);

        // 计算可见范围
        let x_start = screen_origin.x
            - ((screen_origin.x - clip_rect.min.x) / screen_spacing).floor() * screen_spacing;
        let x_end = clip_rect.max.x;
        let y_start = screen_origin.y
            - ((screen_origin.y - clip_rect.min.y) / screen_spacing).floor() * screen_spacing;
        let y_end = clip_rect.max.y;

        // 垂直线
        let mut x = x_start;
        let mut idx = 0i32;
        while x <= x_end {
            let is_axis = (x - screen_origin.x).abs() < 0.5;
            let is_major = show_major && (idx.rem_euclid(major_every as i32) == 0);
            let stroke = if is_axis {
                axis_stroke
            } else if is_major {
                major_stroke
            } else {
                grid_stroke
            };
            painter.line_segment(
                [Pos2::new(x, clip_rect.min.y), Pos2::new(x, clip_rect.max.y)],
                stroke,
            );
            if show_labels && is_major && !is_axis {
                let world_x = (x - screen_origin.x) / self.viewport.zoom;
                painter.text(
                    Pos2::new(x, screen_origin.y + 12.0),
                    egui::Align2::CENTER_TOP,
                    format!("{world_x:.0}"),
                    FontId::proportional(9.0),
                    self.config.label_color,
                );
            }
            x += screen_spacing;
            idx += 1;
        }

        // 水平线
        let mut y = y_start;
        let mut idy = 0i32;
        while y <= y_end {
            let is_axis = (y - screen_origin.y).abs() < 0.5;
            let is_major = show_major && (idy.rem_euclid(major_every as i32) == 0);
            let stroke = if is_axis {
                axis_stroke
            } else if is_major {
                major_stroke
            } else {
                grid_stroke
            };
            painter.line_segment(
                [Pos2::new(clip_rect.min.x, y), Pos2::new(clip_rect.max.x, y)],
                stroke,
            );
            if show_labels && is_major && !is_axis {
                let world_y = -(y - screen_origin.y) / self.viewport.zoom;
                painter.text(
                    Pos2::new(screen_origin.x - 6.0, y),
                    egui::Align2::RIGHT_CENTER,
                    format!("{world_y:.0}"),
                    FontId::proportional(9.0),
                    self.config.label_color,
                );
            }
            y += screen_spacing;
            idy += 1;
        }

        // 原点标签
        if show_labels && clip_rect.contains(screen_origin) {
            painter.text(
                screen_origin,
                egui::Align2::RIGHT_BOTTOM,
                "O",
                FontId::proportional(11.0),
                self.config.label_color,
            );
        }
    }

    /// 绘制点标签
    pub fn draw_point_label(
        &self,
        painter: &Painter,
        world_pos: Point2D,
        canvas_center: Vec2,
        label: &str,
    ) {
        let screen_pos = self.viewport.world_to_screen(world_pos, canvas_center);
        let offset = self.config.point_size * self.viewport.zoom.min(2.0) + 6.0;

        painter.text(
            Pos2::new(screen_pos.x + offset, screen_pos.y - 8.0),
            egui::Align2::LEFT_CENTER,
            label,
            FontId::proportional(12.0),
            self.config.label_color,
        );
    }

    /// 绘制所有几何元素
    ///
    /// 遍历 solver 求解结果，批量绘制点、线、圆。
    #[allow(clippy::too_many_arguments)]
    pub fn draw_all(
        &self,
        painter: &Painter,
        ctx: &SolverContext,
        point_ids: &[Uuid],
        line_defs: &[(Uuid, Uuid, Uuid)], // (line_id, start_id, end_id)
        circle_defs: &[(Uuid, Uuid, f32)], // (circle_id, center_id, radius)
        canvas_center: Vec2,
        selected: Option<Uuid>,
        hovered: Option<Uuid>,
    ) {
        // 先绘制圆（最底层）
        for &(_, center_id, radius) in circle_defs {
            if let Some(center) = ctx.get_2d(center_id) {
                self.draw_circle(
                    painter,
                    center,
                    radius,
                    canvas_center,
                    self.config.circle_color,
                );
            }
        }

        // 再绘制线
        for &(_, start_id, end_id) in line_defs {
            if let (Some(start), Some(end)) = (ctx.get_2d(start_id), ctx.get_2d(end_id)) {
                self.draw_line(painter, start, end, canvas_center, self.config.line_color);
            }
        }

        // 最后绘制点（最上层）
        for &id in point_ids {
            if let Some(pos) = ctx.get_2d(id) {
                if Some(id) == selected {
                    self.draw_point_selected(painter, pos, canvas_center);
                } else if Some(id) == hovered {
                    self.draw_point_hover(painter, pos, canvas_center);
                } else {
                    self.draw_point(painter, pos, canvas_center, self.config.point_color);
                }
            }
        }
    }
}

// ── 贝塞尔曲线辅助函数 ──────────────────────────────────────────

/// 二阶贝塞尔曲线
fn quadratic_bezier(p0: Point2D, p1: Point2D, p2: Point2D, t: f32) -> Point2D {
    let u = 1.0 - t;
    p0 * (u * u) + p1 * (2.0 * u * t) + p2 * (t * t)
}

/// 三阶贝塞尔曲线
fn cubic_bezier(p0: Point2D, p1: Point2D, p2: Point2D, p3: Point2D, t: f32) -> Point2D {
    let u = 1.0 - t;
    p0 * (u * u * u) + p1 * (3.0 * u * u * t) + p2 * (3.0 * u * t * t) + p3 * (t * t * t)
}

// ── 测试 ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_viewport_transform() {
        let vp = Viewport {
            offset: Vec2::ZERO,
            zoom: 1.0,
        };
        let center = Vec2::new(400.0, 300.0);

        let world = Point2D::new(100.0, 50.0);
        let screen = vp.world_to_screen(world, center);
        assert_eq!(screen.x, 500.0);
        assert_eq!(screen.y, 250.0); // Y 翻转

        // 逆变换
        let back = vp.screen_to_world(screen, center);
        assert!((back.x - world.x).abs() < 0.001);
        assert!((back.y - world.y).abs() < 0.001);
    }

    #[test]
    fn test_viewport_zoom() {
        let vp = Viewport {
            offset: Vec2::ZERO,
            zoom: 2.0,
        };
        let center = Vec2::new(400.0, 300.0);

        let world = Point2D::new(50.0, 25.0);
        let screen = vp.world_to_screen(world, center);
        assert_eq!(screen.x, 500.0); // 400 + 50*2
        assert_eq!(screen.y, 250.0); // 300 - 25*2
    }

    #[test]
    fn test_render_config_defaults() {
        let config = RenderConfig::default();
        assert!(config.point_size > 0.0);
        assert!(config.line_width > 0.0);
    }
}
