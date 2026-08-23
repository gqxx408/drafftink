//! 几何查看器 — 自包含 egui 组件
//!
//! 提供完整的动态几何交互体验：
//! - 左侧工具栏：分类图形选择（平面 / 立体 / 标注）
//! - 中央画布：点击/拖拽创建图形
//! - 3D 模式：轨道相机 + 透视/正交投影切换 + 全部立体图形
//! - 持久化：JSON 保存/加载
//!
//! # 交互流程
//! 1. 选择工具 → 点击画布 → 创建图形
//! 2. 多边形/贝塞尔：多点点击 → Enter 完成
//! 3. 圆弧/扇形/椭圆等：点击中心 → 拖拽设定尺寸 → 释放创建
//! 4. 3D 模式：选择立体类型 → 点击添加 → 拖拽旋转观察

use egui::{Color32, Pos2, Stroke, Vec2};
use uuid::Uuid;

use crate::definitions::{Point2D, Point3D, PolyhedronType};
use crate::persistence;
use crate::primitives3d::{
    generate_cone, generate_cube, generate_cuboid, generate_cylinder, generate_frustum,
    generate_prism, generate_pyramid, generate_regular_polyhedron, generate_sphere,
    project_mesh, project_mesh_faces, Camera3D, ProjectedFace, ProjectionMode, RenderMode3D,
};
use crate::renderer::GeometryRenderer;
use crate::seewo_import::{self, SeewoSlide3D};
use crate::solver::GeometrySolver;

// ── 工具类型 ─────────────────────────────────────────────────────

/// 2D 工具类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tool {
    // 基础
    Select,
    Pan,
    Point,
    Line,
    Circle,
    // 多边形系列
    Polygon,
    RegularPolygon,
    Triangle,
    // 圆系扩展
    Arc,
    Sector,
    Ellipse,
    Annulus,
    // 高级曲线
    Bezier,
    // 标注
    AngleMark,
    LengthMark,
    Grid,
}

/// 3D 工具类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tool3D {
    Cube,
    Cuboid,
    Prism,
    Pyramid,
    Sphere,
    Cylinder,
    Cone,
    Frustum,
    Tetrahedron,
    Octahedron,
}

impl Tool3D {
    /// 返回工具的中文名称
    fn label(self) -> &'static str {
        match self {
            Tool3D::Cube => "立方体",
            Tool3D::Cuboid => "长方体",
            Tool3D::Prism => "棱柱",
            Tool3D::Pyramid => "棱锥",
            Tool3D::Sphere => "球体",
            Tool3D::Cylinder => "圆柱",
            Tool3D::Cone => "圆锥",
            Tool3D::Frustum => "圆台",
            Tool3D::Tetrahedron => "正四面体",
            Tool3D::Octahedron => "正八面体",
        }
    }

    /// 返回该工具对应的 3D 基础颜色
    fn base_color(self) -> [u8; 3] {
        match self {
            Tool3D::Cube => [100, 150, 220],
            Tool3D::Cuboid => [120, 180, 200],
            Tool3D::Prism => [100, 200, 180],
            Tool3D::Pyramid => [200, 150, 100],
            Tool3D::Sphere => [120, 200, 150],
            Tool3D::Cylinder => [150, 180, 220],
            Tool3D::Cone => [220, 170, 100],
            Tool3D::Frustum => [180, 140, 220],
            Tool3D::Tetrahedron => [220, 100, 120],
            Tool3D::Octahedron => [100, 220, 200],
        }
    }
}

// ── 交互状态 ─────────────────────────────────────────────────────

/// 交互状态
#[derive(Debug, Clone, Default)]
struct InteractionState {
    // 线段创建
    line_first: Option<Uuid>,
    // 圆创建
    circle_center: Option<Uuid>,
    circle_drag_radius: Option<f32>,
    // 通用拖拽创建（圆弧/扇形/椭圆/圆环/正多边形）
    shape_center: Option<Uuid>,
    shape_drag_radius: Option<f32>,
    // 多边形顶点收集
    polygon_vertices: Vec<Uuid>,
    // 贝塞尔控制点收集
    bezier_points: Vec<Uuid>,
    // 角度标注点收集（vertex, point_a, point_b）
    angle_points: Vec<Uuid>,
    // 长度标注点收集（start, end）
    length_points: Vec<Uuid>,
    // 三角形顶点收集（a, b, c）
    triangle_points: Vec<Uuid>,
    // 拖拽/选中
    dragging: Option<Uuid>,
    hovered: Option<Uuid>,
    selected: Option<Uuid>,
    // 3D
    orbiting: bool,
    #[allow(dead_code)]
    last_mouse: Option<Pos2>,
}

// ── 几何查看器 ───────────────────────────────────────────────────

/// 几何查看器
pub struct GeometryViewer {
    solver: GeometrySolver,
    renderer: GeometryRenderer,
    camera_3d: Camera3D,
    tool: Tool,
    tool_3d: Tool3D,
    is_3d: bool,
    render_mode_3d: RenderMode3D,
    interaction: InteractionState,
    save_path: Option<String>,
    status_msg: String,
    point_counter: usize,
    /// 希沃兼容模式 — 强制纯线框渲染，复刻希沃视觉效果
    seewo_compat_mode: bool,
    /// 导入的希沃 Slide 3D 数据
    seewo_slide: Option<SeewoSlide3D>,
}

impl Default for GeometryViewer {
    fn default() -> Self {
        Self::new()
    }
}

impl GeometryViewer {
    pub fn new() -> Self {
        Self {
            solver: GeometrySolver::new(),
            renderer: GeometryRenderer::new(),
            camera_3d: Camera3D::new(),
            tool: Tool::Point,
            tool_3d: Tool3D::Cube,
            is_3d: false,
            render_mode_3d: RenderMode3D::default(),
            interaction: InteractionState::default(),
            save_path: None,
            status_msg: "选择工具开始绘制".to_string(),
            point_counter: 0,
            seewo_compat_mode: false,
            seewo_slide: None,
        }
    }

    /// 主渲染入口
    pub fn ui(&mut self, ctx: &egui::Context) {
        let _ = self.solver.solve();

        if self.is_3d {
            self.render_3d(ctx);
        } else {
            self.render_2d(ctx);
        }

        self.render_close_button(ctx);
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }

    // ── 2D 渲染 ──────────────────────────────────────────────────

    fn render_2d(&mut self, ctx: &egui::Context) {
        let mut actions = UiActions::default();
        self.render_toolbar_2d(ctx, &mut actions);
        self.render_canvas_2d(ctx, &mut actions);
        self.process_actions_2d(actions, ctx);
    }

    fn render_toolbar_2d(&mut self, ctx: &egui::Context, actions: &mut UiActions) {
        egui::SidePanel::left("geometry_panel_2d")
            .resizable(false)
            .default_width(200.0)
            .min_width(180.0)
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(30, 35, 45))
                    .inner_margin(egui::Margin::same(8.0)),
            )
            .show(ctx, |ui| {
                ui.heading(
                    egui::RichText::new("📐 动态几何")
                        .size(16.0)
                        .strong()
                        .color(Color32::from_rgb(200, 210, 230)),
                );
                ui.add_space(6.0);

                // ── 基础工具 ──
                ui.label(section_label("基础工具"));
                ui.add_space(2.0);
                let basic_tools = [
                    (Tool::Select, "👆 选择"),
                    (Tool::Pan, "✋ 平移"),
                    (Tool::Point, "● 点"),
                    (Tool::Line, "／ 线段"),
                    (Tool::Circle, "○ 圆"),
                ];
                tool_buttons(ui, self.tool, basic_tools, actions);

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // ── 平面图形 ──
                ui.label(section_label("平面图形"));
                ui.add_space(2.0);
                let shape_tools = [
                    (Tool::Polygon, "⬡ 多边形"),
                    (Tool::RegularPolygon, "⬠ 正多边形"),
                    (Tool::Triangle, "△ 三角形"),
                    (Tool::Arc, "◜ 圆弧"),
                    (Tool::Sector, "◔ 扇形"),
                    (Tool::Ellipse, "⬭ 椭圆"),
                    (Tool::Annulus, "◎ 圆环"),
                    (Tool::Bezier, "〰 贝塞尔"),
                ];
                tool_buttons(ui, self.tool, shape_tools, actions);

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // ── 标注 ──
                ui.label(section_label("标注"));
                ui.add_space(2.0);
                let anno_tools = [
                    (Tool::AngleMark, "∠ 角度"),
                    (Tool::LengthMark, "↔ 长度"),
                    (Tool::Grid, "▦ 坐标系"),
                ];
                tool_buttons(ui, self.tool, anno_tools, actions);

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // ── 3D 模式 ──
                if ui.button("立方 3D 模式").clicked() {
                    actions.toggle_3d = true;
                }

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // ── 文件操作 ──
                ui.horizontal(|ui| {
                    if ui.button("💾 保存").clicked() {
                        actions.save = true;
                    }
                    if ui.button("📂 加载").clicked() {
                        actions.load = true;
                    }
                });
                if ui.button("🗑 清空").clicked() {
                    actions.clear = true;
                }

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // ── 统计 ──
                let n_2d = self.solver.doc.count_2d_shapes();
                let n_pts = self.solver.doc.points.len();
                ui.label(
                    egui::RichText::new(format!("点: {n_pts}  图形: {n_2d}"))
                        .size(11.0)
                        .color(Color32::from_rgb(130, 140, 160)),
                );

                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(&self.status_msg)
                        .size(11.0)
                        .color(Color32::from_rgb(180, 190, 210)),
                );

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.label(
                        egui::RichText::new("滚轮缩放 | 右键平移")
                            .size(10.0)
                            .color(Color32::from_rgb(100, 110, 130)),
                    );
                });
            });
    }

    fn render_canvas_2d(&mut self, ctx: &egui::Context, actions: &mut UiActions) {
        let panel_bg = Color32::from_rgb(24, 28, 38);

        let canvas_rect = egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(panel_bg))
            .show(ctx, |ui| ui.max_rect())
            .inner;

        let center = canvas_rect.center();
        let canvas_center = Vec2::new(center.x, center.y);

        let ctx_solved = self.solver.solve().clone();

        // 收集渲染数据
        let point_ids: Vec<Uuid> = self.solver.doc.point_ids();
        let line_defs: Vec<(Uuid, Uuid, Uuid)> = self.solver
            .doc
            .lines
            .values()
            .map(|l| (l.id, l.start, l.end))
            .collect();
        let circle_defs: Vec<(Uuid, Uuid, f32)> = self.solver
            .doc
            .circles
            .values()
            .map(|c| (c.id, c.center, c.radius))
            .collect();

        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("geometry_canvas"),
        ));

        self.renderer.draw_background(&painter, canvas_rect);
        self.renderer.draw_grid(&painter, canvas_rect);

        // ── 绘制扩展 2D 图形 ──
        self.draw_all_2d_shapes(&painter, &ctx_solved, canvas_center);

        // ── 绘制基础元素（点、线、圆）──
        self.renderer.draw_all(
            &painter,
            &ctx_solved,
            &point_ids,
            &line_defs,
            &circle_defs,
            canvas_center,
            self.interaction.selected,
            self.interaction.hovered,
        );

        // ── 创建预览 ──
        self.draw_creation_preview(&painter, &ctx_solved, canvas_center, ctx);

        // ── 交互处理 ──
        self.handle_canvas_interaction_2d(ctx, canvas_rect, canvas_center, &ctx_solved, actions);

        // ── 坐标提示 ──
        let pointer_pos = ctx.input(|i| i.pointer.interact_pos());
        if let Some(mouse) = pointer_pos {
            if canvas_rect.contains(mouse) {
                let world = self.renderer.viewport.screen_to_world(mouse, canvas_center);
                painter.text(
                    Pos2::new(canvas_rect.max.x - 8.0, canvas_rect.max.y - 8.0),
                    egui::Align2::RIGHT_BOTTOM,
                    format!("({:.1}, {:.1})", world.x, world.y),
                    egui::FontId::proportional(12.0),
                    Color32::from_rgb(140, 150, 170),
                );
            }
        }
    }

    /// 绘制所有扩展 2D 图形（多边形、圆弧、扇形、椭圆、圆环、贝塞尔、标注）
    fn draw_all_2d_shapes(
        &self,
        painter: &egui::Painter,
        ctx: &crate::solver::SolverContext,
        canvas_center: Vec2,
    ) {
        let cfg = &self.renderer.config;

        // 多边形
        for poly in self.solver.doc.polygons.values() {
            let pts: Vec<Point2D> = poly
                .vertices
                .iter()
                .filter_map(|&id| ctx.get_2d(id))
                .collect();
            if pts.len() >= 2 {
                self.renderer
                    .draw_polygon(painter, &pts, canvas_center, cfg.polygon_color);
            }
        }

        // 正多边形
        for rp in self.solver.doc.regular_polygons.values() {
            if let Some(center) = ctx.get_2d(rp.center) {
                self.renderer.draw_regular_polygon(
                    painter,
                    center,
                    rp.radius,
                    rp.sides,
                    rp.rotation,
                    canvas_center,
                    cfg.polygon_color,
                );
            }
        }

        // 圆弧
        for arc in self.solver.doc.arcs.values() {
            if let Some(center) = ctx.get_2d(arc.center) {
                self.renderer.draw_arc(
                    painter,
                    center,
                    arc.radius,
                    arc.start_angle,
                    arc.end_angle,
                    canvas_center,
                    cfg.arc_color,
                );
            }
        }

        // 扇形
        for sector in self.solver.doc.sectors.values() {
            if let Some(center) = ctx.get_2d(sector.center) {
                self.renderer.draw_sector(
                    painter,
                    center,
                    sector.radius,
                    sector.start_angle,
                    sector.end_angle,
                    canvas_center,
                    cfg.sector_color,
                );
            }
        }

        // 椭圆
        for ell in self.solver.doc.ellipses.values() {
            if let Some(center) = ctx.get_2d(ell.center) {
                self.renderer.draw_ellipse(
                    painter,
                    center,
                    ell.semi_a,
                    ell.semi_b,
                    ell.rotation,
                    canvas_center,
                    cfg.ellipse_color,
                );
            }
        }

        // 圆环
        for ann in self.solver.doc.annuli.values() {
            if let Some(center) = ctx.get_2d(ann.center) {
                self.renderer.draw_annulus(
                    painter,
                    center,
                    ann.inner_radius,
                    ann.outer_radius,
                    canvas_center,
                    cfg.annulus_color,
                );
            }
        }

        // 贝塞尔曲线
        for bez in self.solver.doc.beziers.values() {
            let pts: Vec<Point2D> = bez
                .control_points
                .iter()
                .filter_map(|&id| ctx.get_2d(id))
                .collect();
            if !pts.is_empty() {
                self.renderer
                    .draw_bezier(painter, &pts, canvas_center, cfg.bezier_color);
            }
        }

        // 角度标注
        for am in self.solver.doc.angle_marks.values() {
            if let (Some(v), Some(a), Some(b)) = (
                ctx.get_2d(am.vertex),
                ctx.get_2d(am.point_a),
                ctx.get_2d(am.point_b),
            ) {
                self.renderer
                    .draw_angle_mark(painter, v, a, b, canvas_center, cfg.annotation_color);
            }
        }

        // 长度标注
        for lm in self.solver.doc.length_marks.values() {
            if let (Some(s), Some(e)) = (ctx.get_2d(lm.start), ctx.get_2d(lm.end)) {
                self.renderer
                    .draw_length_mark(painter, s, e, canvas_center, cfg.annotation_color);
            }
        }

        // 三角形
        for tri in self.solver.doc.triangles.values() {
            if let (Some(a), Some(b), Some(c)) = (
                ctx.get_2d(tri.vertex_a),
                ctx.get_2d(tri.vertex_b),
                ctx.get_2d(tri.vertex_c),
            ) {
                let pts = [a, b, c];
                self.renderer
                    .draw_polygon(painter, &pts, canvas_center, cfg.polygon_color);
            }
        }

        // 坐标系网格
        for grid in self.solver.doc.grids.values() {
            if let Some(origin) = ctx.get_2d(grid.origin) {
                self.renderer.draw_coordinate_grid(
                    painter,
                    origin,
                    grid.spacing,
                    grid.show_major,
                    grid.major_every,
                    grid.show_labels,
                    canvas_center,
                );
            }
        }
    }

    /// 绘制创建中的预览
    fn draw_creation_preview(
        &self,
        painter: &egui::Painter,
        ctx: &crate::solver::SolverContext,
        canvas_center: Vec2,
        ectx: &egui::Context,
    ) {
        let preview_color = Color32::from_rgba_premultiplied(255, 255, 255, 80);

        // 圆/圆弧/扇形/椭圆/圆环/正多边形 拖拽预览
        if let (Some(center_id), Some(radius)) =
            (self.interaction.shape_center, self.interaction.shape_drag_radius)
        {
            if let Some(center_pos) = ctx.get_2d(center_id) {
                match self.tool {
                    Tool::Circle | Tool::Arc | Tool::Sector | Tool::RegularPolygon | Tool::Annulus => {
                        self.renderer
                            .draw_circle(painter, center_pos, radius, canvas_center, preview_color);
                    }
                    Tool::Ellipse => {
                        self.renderer.draw_ellipse(
                            painter,
                            center_pos,
                            radius,
                            radius * 0.6,
                            0.0,
                            canvas_center,
                            preview_color,
                        );
                    }
                    _ => {}
                }
            }
        }

        // 圆创建预览（旧逻辑兼容）
        if let (Some(center_id), Some(radius)) = (
            self.interaction.circle_center,
            self.interaction.circle_drag_radius,
        ) {
            if let Some(center_pos) = ctx.get_2d(center_id) {
                self.renderer.draw_circle(
                    painter,
                    center_pos,
                    radius,
                    canvas_center,
                    preview_color,
                );
            }
        }

        // 线段预览
        if let Some(first_id) = self.interaction.line_first {
            if let Some(first_pos) = ctx.get_2d(first_id) {
                let pointer = ectx.input(|i| i.pointer.interact_pos());
                if let Some(mouse) = pointer {
                    let mouse_world = self.renderer.viewport.screen_to_world(mouse, canvas_center);
                    self.renderer
                        .draw_line(painter, first_pos, mouse_world, canvas_center, preview_color);
                }
            }
        }

        // 多边形预览
        if !self.interaction.polygon_vertices.is_empty() {
            let pts: Vec<Point2D> = self
                .interaction
                .polygon_vertices
                .iter()
                .filter_map(|&id| ctx.get_2d(id))
                .collect();
            if pts.len() >= 2 {
                self.renderer
                    .draw_polygon(painter, &pts, canvas_center, preview_color);
            }
            // 预览到鼠标的连线
            if let Some(last) = pts.last() {
                let pointer = ectx.input(|i| i.pointer.interact_pos());
                if let Some(mouse) = pointer {
                    let mouse_world =
                        self.renderer.viewport.screen_to_world(mouse, canvas_center);
                    self.renderer
                        .draw_line(painter, *last, mouse_world, canvas_center, preview_color);
                }
            }
        }

        // 贝塞尔预览
        if !self.interaction.bezier_points.is_empty() {
            let pts: Vec<Point2D> = self
                .interaction
                .bezier_points
                .iter()
                .filter_map(|&id| ctx.get_2d(id))
                .collect();
            if pts.len() >= 2 {
                self.renderer
                    .draw_bezier(painter, &pts, canvas_center, preview_color);
            }
        }

        // 三角形预览
        if !self.interaction.triangle_points.is_empty() {
            let pts: Vec<Point2D> = self
                .interaction
                .triangle_points
                .iter()
                .filter_map(|&id| ctx.get_2d(id))
                .collect();
            if pts.len() >= 2 {
                self.renderer
                    .draw_polygon(painter, &pts, canvas_center, preview_color);
            }
            // 预览到鼠标的连线
            if let Some(last) = pts.last() {
                let pointer = ectx.input(|i| i.pointer.interact_pos());
                if let Some(mouse) = pointer {
                    let mouse_world =
                        self.renderer.viewport.screen_to_world(mouse, canvas_center);
                    self.renderer
                        .draw_line(painter, *last, mouse_world, canvas_center, preview_color);
                }
            }
        }
    }

    /// 处理画布交互
    fn handle_canvas_interaction_2d(
        &mut self,
        ctx: &egui::Context,
        canvas_rect: egui::Rect,
        canvas_center: Vec2,
        ctx_solved: &crate::solver::SolverContext,
        actions: &mut UiActions,
    ) {
        let pointer_pos = ctx.input(|i| i.pointer.interact_pos());
        let primary_down = ctx.input(|i| i.pointer.primary_down());
        let primary_clicked = ctx.input(|i| i.pointer.primary_clicked());
        let primary_released = ctx.input(|i| i.pointer.primary_released());
        let drag_delta = ctx.input(|i| i.pointer.delta());

        // 滚轮缩放
        let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.5 {
            self.renderer.viewport.zoom =
                (self.renderer.viewport.zoom * (1.0 + scroll * 0.001)).clamp(0.1, 10.0);
        }

        // 右键平移
        let secondary_down = ctx.input(|i| i.pointer.secondary_down());
        if secondary_down && drag_delta != Vec2::ZERO {
            self.renderer.viewport.offset += drag_delta;
        }

        // 删除选中
        if ctx.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
            if let Some(id) = self.interaction.selected {
                actions.delete_element = Some(id);
            }
        }

        // Enter 完成多边形/贝塞尔
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            actions.finish_multi_click = true;
        }

        if let Some(mouse) = pointer_pos {
            if !canvas_rect.contains(mouse) {
                return;
            }
            let mouse_world = self.renderer.viewport.screen_to_world(mouse, canvas_center);

            // 悬停检测
            self.interaction.hovered =
                self.find_point_at(mouse_world, ctx_solved, canvas_center, mouse);

            match self.tool {
                Tool::Point => {
                    if primary_clicked {
                        actions.add_point = Some(mouse_world);
                    }
                }
                Tool::Line => {
                    if primary_clicked {
                        let target = self.interaction.hovered.or_else(|| {
                            let id = self.solver.add_free_point(mouse_world);
                            self.point_counter += 1;
                            Some(id)
                        });
                        actions.line_click = target;
                    }
                }
                Tool::Circle => {
                    self.handle_drag_create(
                        primary_clicked,
                        primary_down,
                        primary_released,
                        mouse_world,
                        ctx_solved,
                        |center_id, radius| actions.circle_create = Some((center_id, radius)),
                    );
                }
                Tool::Polygon => {
                    if primary_clicked {
                        let target = self.interaction.hovered.or_else(|| {
                            let id = self.solver.add_free_point(mouse_world);
                            Some(id)
                        });
                        if let Some(id) = target {
                            self.interaction.polygon_vertices.push(id);
                            self.status_msg = format!(
                                "多边形顶点: {} (Enter 完成)",
                                self.interaction.polygon_vertices.len()
                            );
                        }
                    }
                }
                Tool::RegularPolygon => {
                    self.handle_drag_create(
                        primary_clicked,
                        primary_down,
                        primary_released,
                        mouse_world,
                        ctx_solved,
                        |center_id, radius| actions.regular_polygon_create = Some((center_id, radius)),
                    );
                }
                Tool::Arc => {
                    self.handle_drag_create(
                        primary_clicked,
                        primary_down,
                        primary_released,
                        mouse_world,
                        ctx_solved,
                        |center_id, radius| actions.arc_create = Some((center_id, radius)),
                    );
                }
                Tool::Sector => {
                    self.handle_drag_create(
                        primary_clicked,
                        primary_down,
                        primary_released,
                        mouse_world,
                        ctx_solved,
                        |center_id, radius| actions.sector_create = Some((center_id, radius)),
                    );
                }
                Tool::Ellipse => {
                    self.handle_drag_create(
                        primary_clicked,
                        primary_down,
                        primary_released,
                        mouse_world,
                        ctx_solved,
                        |center_id, radius| actions.ellipse_create = Some((center_id, radius)),
                    );
                }
                Tool::Annulus => {
                    self.handle_drag_create(
                        primary_clicked,
                        primary_down,
                        primary_released,
                        mouse_world,
                        ctx_solved,
                        |center_id, radius| actions.annulus_create = Some((center_id, radius)),
                    );
                }
                Tool::Bezier => {
                    if primary_clicked {
                        let target = self.interaction.hovered.or_else(|| {
                            let id = self.solver.add_free_point(mouse_world);
                            Some(id)
                        });
                        if let Some(id) = target {
                            self.interaction.bezier_points.push(id);
                            let n = self.interaction.bezier_points.len();
                            if n == 3 {
                                self.status_msg = "二阶贝塞尔已就绪 (Enter 完成)".into();
                            } else if n == 4 {
                                self.status_msg = "三阶贝塞尔已就绪 (Enter 完成)".into();
                            } else {
                                self.status_msg = format!("控制点: {n} (3=二阶, 4=三阶)");
                            }
                        }
                    }
                }
                Tool::AngleMark => {
                    if primary_clicked {
                        let target = self.interaction.hovered.or_else(|| {
                            let id = self.solver.add_free_point(mouse_world);
                            Some(id)
                        });
                        if let Some(id) = target {
                            self.interaction.angle_points.push(id);
                            let n = self.interaction.angle_points.len();
                            self.status_msg = match n {
                                1 => "选择第一条边上的点".into(),
                                2 => "选择第二条边上的点".into(),
                                _ => "角度标注已创建".into(),
                            };
                            if n >= 3 {
                                actions.angle_mark_create = true;
                            }
                        }
                    }
                }
                Tool::LengthMark => {
                    if primary_clicked {
                        let target = self.interaction.hovered.or_else(|| {
                            let id = self.solver.add_free_point(mouse_world);
                            Some(id)
                        });
                        if let Some(id) = target {
                            self.interaction.length_points.push(id);
                            let n = self.interaction.length_points.len();
                            if n == 1 {
                                self.status_msg = "选择终点".into();
                            } else {
                                self.status_msg = "长度标注已创建".into();
                            }
                            if n >= 2 {
                                actions.length_mark_create = true;
                            }
                        }
                    }
                }
                Tool::Triangle => {
                    if primary_clicked {
                        let target = self.interaction.hovered.or_else(|| {
                            let id = self.solver.add_free_point(mouse_world);
                            Some(id)
                        });
                        if let Some(id) = target {
                            self.interaction.triangle_points.push(id);
                            let n = self.interaction.triangle_points.len();
                            self.status_msg = match n {
                                1 => "选择第二个顶点".into(),
                                2 => "选择第三个顶点".into(),
                                _ => "三角形已创建".into(),
                            };
                            if n >= 3 {
                                actions.triangle_create = true;
                            }
                        }
                    }
                }
                Tool::Grid => {
                    if primary_clicked {
                        actions.grid_create = Some(mouse_world);
                    }
                }
                Tool::Select => {
                    if primary_clicked {
                        self.interaction.selected = self.interaction.hovered;
                        self.interaction.dragging = self.interaction.hovered;
                    }
                    if primary_released {
                        self.interaction.dragging = None;
                    }
                    if primary_down && drag_delta != Vec2::ZERO {
                        if let Some(drag_id) = self.interaction.dragging {
                            actions.drag_point = Some((drag_id, mouse_world));
                        }
                    }
                }
                Tool::Pan => {
                    if primary_down && drag_delta != Vec2::ZERO {
                        self.renderer.viewport.offset += drag_delta;
                    }
                }
            }
        }
    }

    /// 通用拖拽创建逻辑（圆/圆弧/扇形/椭圆/圆环/正多边形）
    fn handle_drag_create(
        &mut self,
        primary_clicked: bool,
        primary_down: bool,
        primary_released: bool,
        mouse_world: Point2D,
        ctx_solved: &crate::solver::SolverContext,
        mut on_create: impl FnMut(Uuid, f32),
    ) {
        if primary_clicked {
            let target = self.interaction.hovered.or_else(|| {
                let id = self.solver.add_free_point(mouse_world);
                self.point_counter += 1;
                Some(id)
            });
            self.interaction.shape_center = target;
            self.interaction.shape_drag_radius = Some(0.0);
            self.status_msg = "拖拽设定尺寸".into();
        }
        if primary_down && self.interaction.shape_center.is_some() {
            if let Some(center_id) = self.interaction.shape_center {
                if let Some(center_pos) = ctx_solved.get_2d(center_id) {
                    let dist = (mouse_world - center_pos).norm();
                    self.interaction.shape_drag_radius = Some(dist);
                }
            }
        }
        if primary_released && self.interaction.shape_center.is_some() {
            if let Some(radius) = self.interaction.shape_drag_radius {
                if radius > 2.0 {
                    if let Some(center_id) = self.interaction.shape_center {
                        on_create(center_id, radius);
                    }
                }
            }
            self.interaction.shape_center = None;
            self.interaction.shape_drag_radius = None;
        }
    }

    // ── 3D 渲染 ──────────────────────────────────────────────────

    fn render_3d(&mut self, ctx: &egui::Context) {
        let mut actions = UiActions3D::default();
        self.render_toolbar_3d(ctx, &mut actions);
        self.render_canvas_3d(ctx, &mut actions);
        self.process_actions_3d(actions);
    }

    fn render_toolbar_3d(&mut self, ctx: &egui::Context, actions: &mut UiActions3D) {
        egui::SidePanel::left("geometry_panel_3d")
            .resizable(false)
            .default_width(200.0)
            .min_width(180.0)
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(30, 35, 45))
                    .inner_margin(egui::Margin::same(8.0)),
            )
            .show(ctx, |ui| {
                ui.heading(
                    egui::RichText::new("立方 3D 立体图形")
                        .size(16.0)
                        .strong()
                        .color(Color32::from_rgb(200, 210, 230)),
                );
                ui.add_space(6.0);

                // ── 多面体 ──
                ui.label(section_label("多面体"));
                ui.add_space(2.0);
                let poly_tools = [
                    (Tool3D::Cube, "立方体"),
                    (Tool3D::Cuboid, "长方体"),
                    (Tool3D::Prism, "棱柱"),
                    (Tool3D::Pyramid, "棱锥"),
                    (Tool3D::Tetrahedron, "正四面体"),
                    (Tool3D::Octahedron, "正八面体"),
                ];
                tool3d_buttons(ui, self.tool_3d, poly_tools, actions);

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // ── 曲面体 ──
                ui.label(section_label("曲面体"));
                ui.add_space(2.0);
                let curved_tools = [
                    (Tool3D::Sphere, "球体"),
                    (Tool3D::Cylinder, "圆柱"),
                    (Tool3D::Cone, "圆锥"),
                    (Tool3D::Frustum, "圆台"),
                ];
                tool3d_buttons(ui, self.tool_3d, curved_tools, actions);

                ui.add_space(4.0);
                if ui.button("＋ 添加立体图形").clicked() {
                    actions.add_primitive = true;
                }

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // ── 投影模式 ──
                ui.label(section_label("投影"));
                let proj_label = match self.camera_3d.projection {
                    ProjectionMode::Perspective => "透视投影",
                    ProjectionMode::Orthographic => "正交投影",
                };
                if ui.button(proj_label).clicked() {
                    actions.toggle_projection = true;
                }
                if ui.button("重置视角").clicked() {
                    actions.reset_camera = true;
                }

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // ── 渲染模式 ──
                ui.label(section_label("渲染模式"));
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(self.render_mode_3d == RenderMode3D::Wireframe, "线框")
                        .clicked()
                    {
                        actions.render_mode = Some(RenderMode3D::Wireframe);
                    }
                    if ui
                        .selectable_label(self.render_mode_3d == RenderMode3D::Solid, "实心")
                        .clicked()
                    {
                        actions.render_mode = Some(RenderMode3D::Solid);
                    }
                    if ui
                        .selectable_label(
                            self.render_mode_3d == RenderMode3D::SolidWireframe,
                            "实心+线框",
                        )
                        .clicked()
                    {
                        actions.render_mode = Some(RenderMode3D::SolidWireframe);
                    }
                });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // ── 希沃兼容 ──
                ui.label(section_label("希沃 EasiNote"));
                ui.add_space(2.0);
                if ui
                    .checkbox(&mut self.seewo_compat_mode, "希沃兼容模式")
                    .changed()
                {
                    actions.toggle_seewo_compat = true;
                }
                if ui.button("📥 导入希沃 XML").clicked() {
                    actions.import_seewo_xml = true;
                }
                if self.seewo_slide.is_some() {
                    let n = self.seewo_slide.as_ref().unwrap().cylinders.len()
                        + self.seewo_slide.as_ref().unwrap().cones.len();
                    ui.label(
                        egui::RichText::new(format!("已加载 {n} 个 3D 对象"))
                            .size(10.0)
                            .color(Color32::from_rgb(100, 200, 100)),
                    );
                }

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // ── 返回 2D ──
                if ui.button("← 返回 2D").clicked() {
                    actions.back_to_2d = true;
                }

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // ── 统计 ──
                let n_3d = self.solver.doc.count_3d_shapes();
                ui.label(
                    egui::RichText::new(format!("立体图形: {n_3d}"))
                        .size(11.0)
                        .color(Color32::from_rgb(130, 140, 160)),
                );

                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(&self.status_msg)
                        .size(11.0)
                        .color(Color32::from_rgb(180, 190, 210)),
                );

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.label(
                        egui::RichText::new("拖拽旋转 | 滚轮缩放")
                            .size(10.0)
                            .color(Color32::from_rgb(100, 110, 130)),
                    );
                });
            });
    }

    fn render_canvas_3d(&mut self, ctx: &egui::Context, _actions: &mut UiActions3D) {
        let panel_bg = Color32::from_rgb(20, 22, 32);

        let canvas_rect = egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(panel_bg))
            .show(ctx, |ui| ui.max_rect())
            .inner;

        let aspect = canvas_rect.width() / canvas_rect.height().max(1.0);
        let screen_size = (canvas_rect.width(), canvas_rect.height());

        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("geometry_canvas_3d"),
        ));

        // 地面网格
        self.draw_3d_ground_grid(&painter, canvas_rect, aspect, screen_size);

        let ctx_solved = self.solver.solve().clone();

        // 收集所有 3D 网格
        let mut all_faces: Vec<ProjectedFace> = Vec::new();
        let mut all_edges: Vec<crate::primitives3d::ProjectedEdge> = Vec::new();

        // 立方体
        for cube in self.solver.doc.cubes.values() {
            if let Some(center) = ctx_solved.get_3d(cube.center) {
                let mesh = generate_cube(center, cube.size);
                all_faces.extend(project_mesh_faces(&mesh, &self.camera_3d, aspect, screen_size, Tool3D::Cube.base_color()));
                all_edges.extend(project_mesh(&mesh, &self.camera_3d, aspect, screen_size));
            }
        }

        // 长方体
        for cuboid in self.solver.doc.cuboids.values() {
            if let Some(center) = ctx_solved.get_3d(cuboid.center) {
                let mesh = generate_cuboid(center, cuboid.width, cuboid.height, cuboid.depth);
                all_faces.extend(project_mesh_faces(&mesh, &self.camera_3d, aspect, screen_size, Tool3D::Cuboid.base_color()));
                all_edges.extend(project_mesh(&mesh, &self.camera_3d, aspect, screen_size));
            }
        }

        // 球体
        for sphere in self.solver.doc.spheres.values() {
            if let Some(center) = ctx_solved.get_3d(sphere.center) {
                let mesh = generate_sphere(center, sphere.radius, 12, 16);
                all_faces.extend(project_mesh_faces(&mesh, &self.camera_3d, aspect, screen_size, Tool3D::Sphere.base_color()));
                all_edges.extend(project_mesh(&mesh, &self.camera_3d, aspect, screen_size));
            }
        }

        // 圆柱
        for cyl in self.solver.doc.cylinders.values() {
            if let (Some(bot), Some(top)) =
                (ctx_solved.get_3d(cyl.bottom_center), ctx_solved.get_3d(cyl.top_center))
            {
                let mesh = generate_cylinder(bot, top, cyl.radius, 16);
                all_faces.extend(project_mesh_faces(&mesh, &self.camera_3d, aspect, screen_size, Tool3D::Cylinder.base_color()));
                all_edges.extend(project_mesh(&mesh, &self.camera_3d, aspect, screen_size));
            }
        }

        // 圆锥
        for cone in self.solver.doc.cones.values() {
            if let (Some(base), Some(apex)) =
                (ctx_solved.get_3d(cone.base_center), ctx_solved.get_3d(cone.apex))
            {
                let mesh = generate_cone(base, apex, cone.radius, 16);
                all_faces.extend(project_mesh_faces(&mesh, &self.camera_3d, aspect, screen_size, Tool3D::Cone.base_color()));
                all_edges.extend(project_mesh(&mesh, &self.camera_3d, aspect, screen_size));
            }
        }

        // 圆台
        for frust in self.solver.doc.frustums.values() {
            if let (Some(bot), Some(top)) = (
                ctx_solved.get_3d(frust.bottom_center),
                ctx_solved.get_3d(frust.top_center),
            ) {
                let mesh = generate_frustum(bot, top, frust.bottom_radius, frust.top_radius, 16);
                all_faces.extend(project_mesh_faces(&mesh, &self.camera_3d, aspect, screen_size, Tool3D::Frustum.base_color()));
                all_edges.extend(project_mesh(&mesh, &self.camera_3d, aspect, screen_size));
            }
        }

        // 棱柱
        for prism in self.solver.doc.prisms.values() {
            if let (Some(bot), Some(top)) = (
                ctx_solved.get_3d(prism.base_center),
                ctx_solved.get_3d(prism.top_center),
            ) {
                let mesh = generate_prism(bot, top, prism.radius, prism.sides);
                all_faces.extend(project_mesh_faces(&mesh, &self.camera_3d, aspect, screen_size, Tool3D::Prism.base_color()));
                all_edges.extend(project_mesh(&mesh, &self.camera_3d, aspect, screen_size));
            }
        }

        // 棱锥
        for pyr in self.solver.doc.pyramids.values() {
            if let (Some(base), Some(apex)) =
                (ctx_solved.get_3d(pyr.base_center), ctx_solved.get_3d(pyr.apex))
            {
                let mesh = generate_pyramid(base, apex, pyr.radius, pyr.sides);
                all_faces.extend(project_mesh_faces(&mesh, &self.camera_3d, aspect, screen_size, Tool3D::Pyramid.base_color()));
                all_edges.extend(project_mesh(&mesh, &self.camera_3d, aspect, screen_size));
            }
        }

        // 正多面体
        for rp in self.solver.doc.regular_polyhedra.values() {
            if let Some(center) = ctx_solved.get_3d(rp.center) {
                let mesh = generate_regular_polyhedron(center, rp.size, rp.poly_type);
                let color = match rp.poly_type {
                    PolyhedronType::Tetrahedron => Tool3D::Tetrahedron.base_color(),
                    PolyhedronType::Octahedron => Tool3D::Octahedron.base_color(),
                    PolyhedronType::Hexahedron => Tool3D::Cube.base_color(),
                    PolyhedronType::Icosahedron => [100, 200, 255],
                    PolyhedronType::Dodecahedron => [200, 100, 200],
                };
                all_faces.extend(project_mesh_faces(&mesh, &self.camera_3d, aspect, screen_size, color));
                all_edges.extend(project_mesh(&mesh, &self.camera_3d, aspect, screen_size));
            }
        }

        // ── 希沃 EasiNote 导入对象 ──
        // 收集希沃网格，分别存储边和面以便兼容模式渲染
        let mut seewo_edges: Vec<(crate::primitives3d::ProjectedEdge, Color32, f32)> = Vec::new();
        let mut seewo_faces: Vec<ProjectedFace> = Vec::new();

        if let Some(slide) = &self.seewo_slide {
            let meshes = seewo_import::collect_all_meshes(slide);
            let slide_w = 1280.0_f32;
            let slide_h = 720.0_f32;
            let scale_x = screen_size.0 / slide_w;
            let scale_y = screen_size.1 / slide_h;

            for mesh_data in meshes {
                // 投影面（实心模式用）— 石膏体白色基底
                let mut faces = project_mesh_faces(&mesh_data.mesh, &self.camera_3d, aspect, screen_size, [220, 220, 230]);
                // 投影边
                let mut edges = project_mesh(&mesh_data.mesh, &self.camera_3d, aspect, screen_size);

                // 计算投影后的中心点，用于屏幕偏移定位
                let mut min_x = f32::MAX;
                let mut min_y = f32::MAX;
                let mut max_x = f32::MIN;
                let mut max_y = f32::MIN;
                for e in &edges {
                    min_x = min_x.min(e.start.0).min(e.end.0);
                    min_y = min_y.min(e.start.1).min(e.end.1);
                    max_x = max_x.max(e.start.0).max(e.end.0);
                    max_y = max_y.max(e.start.1).max(e.end.1);
                }
                let centroid_x = (min_x + max_x) * 0.5;
                let centroid_y = (min_y + max_y) * 0.5;

                // 希沃屏幕坐标 → 画布坐标
                let target_x = mesh_data.screen_x * scale_x;
                let target_y = mesh_data.screen_y * scale_y;
                let offset_x = target_x - centroid_x;
                let offset_y = target_y - centroid_y;

                // 应用偏移
                for e in &mut edges {
                    e.start.0 += offset_x;
                    e.start.1 += offset_y;
                    e.end.0 += offset_x;
                    e.end.1 += offset_y;
                }
                for f in &mut faces {
                    for v in &mut f.vertices {
                        v.0 += offset_x;
                        v.1 += offset_y;
                    }
                }

                let stroke_color = Color32::from_rgba_premultiplied(
                    mesh_data.edge_color.1,
                    mesh_data.edge_color.2,
                    mesh_data.edge_color.3,
                    mesh_data.edge_color.0,
                );
                for e in edges {
                    seewo_edges.push((e, stroke_color, mesh_data.edge_thickness));
                }
                seewo_faces.extend(faces);
            }
        }

        // 按渲染模式绘制
        let effective_mode = if self.seewo_compat_mode {
            RenderMode3D::Wireframe
        } else {
            self.render_mode_3d
        };

        match effective_mode {
            RenderMode3D::Wireframe => {
                // 希沃兼容模式：纯黑线框，完美复刻希沃
                if self.seewo_compat_mode {
                    for (edge, color, thickness) in &seewo_edges {
                        let stroke = Stroke::new(*thickness, *color);
                        painter.line_segment(
                            [Pos2::new(edge.start.0, edge.start.1), Pos2::new(edge.end.0, edge.end.1)],
                            stroke,
                        );
                    }
                }
                // 常规线框
                let stroke = Stroke::new(1.5_f32, Color32::from_rgb(100, 180, 255));
                for edge in &all_edges {
                    painter.line_segment(
                        [Pos2::new(edge.start.0, edge.start.1), Pos2::new(edge.end.0, edge.end.1)],
                        stroke,
                    );
                }
            }
            RenderMode3D::Solid => {
                for face in &all_faces {
                    let color = Color32::from_rgba_premultiplied(
                        face.color[0],
                        face.color[1],
                        face.color[2],
                        face.color[3],
                    );
                    let pts = [
                        Pos2::new(face.vertices[0].0, face.vertices[0].1),
                        Pos2::new(face.vertices[1].0, face.vertices[1].1),
                        Pos2::new(face.vertices[2].0, face.vertices[2].1),
                    ];
                    painter.add(egui::Shape::convex_polygon(pts.to_vec(), color, Stroke::NONE));
                }
                // 希沃实心面（石膏体）
                for face in &seewo_faces {
                    let color = Color32::from_rgba_premultiplied(
                        face.color[0],
                        face.color[1],
                        face.color[2],
                        face.color[3],
                    );
                    let pts = [
                        Pos2::new(face.vertices[0].0, face.vertices[0].1),
                        Pos2::new(face.vertices[1].0, face.vertices[1].1),
                        Pos2::new(face.vertices[2].0, face.vertices[2].1),
                    ];
                    painter.add(egui::Shape::convex_polygon(pts.to_vec(), color, Stroke::NONE));
                }
            }
            RenderMode3D::SolidWireframe => {
                for face in &all_faces {
                    let color = Color32::from_rgba_premultiplied(
                        face.color[0],
                        face.color[1],
                        face.color[2],
                        face.color[3],
                    );
                    let pts = [
                        Pos2::new(face.vertices[0].0, face.vertices[0].1),
                        Pos2::new(face.vertices[1].0, face.vertices[1].1),
                        Pos2::new(face.vertices[2].0, face.vertices[2].1),
                    ];
                    painter.add(egui::Shape::convex_polygon(pts.to_vec(), color, Stroke::NONE));
                }
                // 希沃实心面（石膏体）
                for face in &seewo_faces {
                    let color = Color32::from_rgba_premultiplied(
                        face.color[0],
                        face.color[1],
                        face.color[2],
                        face.color[3],
                    );
                    let pts = [
                        Pos2::new(face.vertices[0].0, face.vertices[0].1),
                        Pos2::new(face.vertices[1].0, face.vertices[1].1),
                        Pos2::new(face.vertices[2].0, face.vertices[2].1),
                    ];
                    painter.add(egui::Shape::convex_polygon(pts.to_vec(), color, Stroke::NONE));
                }
                // 常规边线
                let stroke = Stroke::new(1.0_f32, Color32::from_rgb(40, 50, 70));
                for edge in &all_edges {
                    painter.line_segment(
                        [Pos2::new(edge.start.0, edge.start.1), Pos2::new(edge.end.0, edge.end.1)],
                        stroke,
                    );
                }
                // 希沃边线（黑色 2px，叠加在实心面上）
                for (edge, color, thickness) in &seewo_edges {
                    let stroke = Stroke::new(*thickness, *color);
                    painter.line_segment(
                        [Pos2::new(edge.start.0, edge.start.1), Pos2::new(edge.end.0, edge.end.1)],
                        stroke,
                    );
                }
            }
        }

        // 3D 交互
        let pointer_pos = ctx.input(|i| i.pointer.interact_pos());
        let primary_down = ctx.input(|i| i.pointer.primary_down());
        let primary_released = ctx.input(|i| i.pointer.primary_released());
        let drag_delta = ctx.input(|i| i.pointer.delta());

        if let Some(mouse) = pointer_pos {
            if canvas_rect.contains(mouse) {
                if primary_down && drag_delta != Vec2::ZERO {
                    self.camera_3d.orbit(drag_delta.x, drag_delta.y);
                    self.interaction.orbiting = true;
                }
                if primary_released {
                    self.interaction.orbiting = false;
                }
            }
        }

        let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.5 {
            self.camera_3d.zoom(scroll);
        }

        if let Some(mouse) = pointer_pos {
            if canvas_rect.contains(mouse) {
                painter.text(
                    Pos2::new(canvas_rect.max.x - 8.0, canvas_rect.max.y - 8.0),
                    egui::Align2::RIGHT_BOTTOM,
                    format!(
                        "dist: {:.1}  proj: {}",
                        self.camera_3d.distance,
                        match self.camera_3d.projection {
                            ProjectionMode::Perspective => "Persp",
                            ProjectionMode::Orthographic => "Ortho",
                        }
                    ),
                    egui::FontId::proportional(12.0),
                    Color32::from_rgb(140, 150, 170),
                );
            }
        }
    }

    /// 绘制 3D 地面参考网格
    fn draw_3d_ground_grid(
        &self,
        painter: &egui::Painter,
        _canvas_rect: egui::Rect,
        aspect: f32,
        screen_size: (f32, f32),
    ) {
        let grid_size = 5.0_f32;
        let grid_step = 1.0_f32;
        let grid_color = Color32::from_rgba_premultiplied(60, 65, 80, 60);

        let mut edges: Vec<crate::primitives3d::ProjectedEdge> = Vec::new();

        let mut x = -grid_size;
        while x <= grid_size {
            let start = self.camera_3d.project(Point3D::new(x, 0.0, -grid_size), aspect, screen_size);
            let end = self.camera_3d.project(Point3D::new(x, 0.0, grid_size), aspect, screen_size);
            if let (Some(s), Some(e)) = (start, end) {
                edges.push(crate::primitives3d::ProjectedEdge { start: s, end: e, depth: 0.0 });
            }
            x += grid_step;
        }

        let mut z = -grid_size;
        while z <= grid_size {
            let start = self.camera_3d.project(Point3D::new(-grid_size, 0.0, z), aspect, screen_size);
            let end = self.camera_3d.project(Point3D::new(grid_size, 0.0, z), aspect, screen_size);
            if let (Some(s), Some(e)) = (start, end) {
                edges.push(crate::primitives3d::ProjectedEdge { start: s, end: e, depth: 0.0 });
            }
            z += grid_step;
        }

        let stroke = Stroke::new(0.5_f32, grid_color);
        for edge in &edges {
            painter.line_segment(
                [Pos2::new(edge.start.0, edge.start.1), Pos2::new(edge.end.0, edge.end.1)],
                stroke,
            );
        }
    }

    // ── 辅助方法 ─────────────────────────────────────────────────

    /// 查找鼠标位置附近的点
    fn find_point_at(
        &self,
        world_pos: Point2D,
        ctx: &crate::solver::SolverContext,
        canvas_center: Vec2,
        mouse_screen: Pos2,
    ) -> Option<Uuid> {
        let hit_radius = 12.0_f32;
        let mut closest: Option<(Uuid, f32)> = None;

        for &id in self.solver.doc.points.keys() {
            if let Some(pos) = ctx.get_2d(id) {
                let screen = self.renderer.viewport.world_to_screen(pos, canvas_center);
                let dist = ((screen.x - mouse_screen.x).powi(2)
                    + (screen.y - mouse_screen.y).powi(2))
                .sqrt();
                if dist < hit_radius {
                    let is_closer = match closest {
                        Some((_, d)) => dist < d,
                        None => true,
                    };
                    if is_closer {
                        closest = Some((id, dist));
                    }
                }
            }
        }

        if closest.is_none() {
            for &id in self.solver.doc.points.keys() {
                if let Some(pos) = ctx.get_2d(id) {
                    let dist = (pos - world_pos).norm();
                    if dist < hit_radius / self.renderer.viewport.zoom {
                        let is_closer = match closest {
                            Some((_, d)) => dist < d,
                            None => true,
                        };
                        if is_closer {
                            closest = Some((id, dist));
                        }
                    }
                }
            }
        }

        closest.map(|(id, _)| id)
    }

    // ── 动作处理 ─────────────────────────────────────────────────

    fn process_actions_2d(&mut self, actions: UiActions, ctx: &egui::Context) {
        if let Some(tool) = actions.tool_changed {
            self.tool = tool;
            // 重置多重点击状态
            self.interaction.polygon_vertices.clear();
            self.interaction.bezier_points.clear();
            self.interaction.angle_points.clear();
            self.interaction.length_points.clear();
            self.interaction.triangle_points.clear();
            self.interaction.line_first = None;
            self.interaction.circle_center = None;
            self.interaction.circle_drag_radius = None;
            self.interaction.shape_center = None;
            self.interaction.shape_drag_radius = None;
            self.status_msg = match tool {
                Tool::Select => "点击选择点，拖拽移动".into(),
                Tool::Pan => "拖拽平移画布".into(),
                Tool::Point => "点击画布添加点".into(),
                Tool::Line => "点击两个点创建线段".into(),
                Tool::Circle => "点击中心点，拖拽设定半径".into(),
                Tool::Polygon => "点击添加顶点，Enter 完成".into(),
                Tool::RegularPolygon => "点击中心，拖拽设定半径".into(),
                Tool::Arc => "点击中心，拖拽设定半径".into(),
                Tool::Sector => "点击中心，拖拽设定半径".into(),
                Tool::Ellipse => "点击中心，拖拽设定半轴".into(),
                Tool::Annulus => "点击中心，拖拽设定外半径".into(),
                Tool::Bezier => "点击 3 点(二阶)或 4 点(三阶)".into(),
                Tool::AngleMark => "点击顶点、两边上的点".into(),
                Tool::LengthMark => "点击起点和终点".into(),
                Tool::Triangle => "点击三个顶点创建三角形".into(),
                Tool::Grid => "点击放置坐标系网格".into(),
            };
        }

        if actions.toggle_3d {
            self.is_3d = true;
            self.status_msg = "3D 模式：拖拽旋转，滚轮缩放".into();
        }

        if let Some(pos) = actions.add_point {
            self.solver.add_free_point(pos);
            self.point_counter += 1;
            self.status_msg = format!("已添加点 P{}", self.point_counter);
        }

        if let Some(target) = actions.line_click {
            if let Some(first) = self.interaction.line_first {
                if first != target {
                    match self.solver.add_line(first, target) {
                        Ok(_) => self.status_msg = "线段已创建".into(),
                        Err(e) => self.status_msg = format!("错误: {e}"),
                    }
                }
                self.interaction.line_first = None;
            } else {
                self.interaction.line_first = Some(target);
                self.status_msg = "选择第二个点".into();
            }
        }

        // 圆创建
        if let Some((center_id, radius)) = actions.circle_create {
            match self.solver.add_circle(center_id, radius) {
                Ok(_) => self.status_msg = format!("圆已创建 (r={radius:.1})"),
                Err(e) => self.status_msg = format!("错误: {e}"),
            }
        }

        // 正多边形创建
        if let Some((center_id, radius)) = actions.regular_polygon_create {
            self.solver.doc.add_regular_polygon(center_id, radius, 6, 0.0);
            self.solver.mark_dirty();
            self.status_msg = format!("正六边形已创建 (r={radius:.1})");
        }

        // 圆弧创建
        if let Some((center_id, radius)) = actions.arc_create {
            self.solver.doc.add_arc(center_id, radius, 0.0, std::f32::consts::FRAC_PI_2);
            self.solver.mark_dirty();
            self.status_msg = format!("圆弧已创建 (r={radius:.1})");
        }

        // 扇形创建
        if let Some((center_id, radius)) = actions.sector_create {
            self.solver.doc.add_sector(center_id, radius, 0.0, std::f32::consts::FRAC_PI_2);
            self.solver.mark_dirty();
            self.status_msg = format!("扇形已创建 (r={radius:.1})");
        }

        // 椭圆创建
        if let Some((center_id, radius)) = actions.ellipse_create {
            self.solver.doc.add_ellipse(center_id, radius, radius * 0.6, 0.0);
            self.solver.mark_dirty();
            self.status_msg = format!("椭圆已创建 (a={radius:.1})");
        }

        // 圆环创建
        if let Some((center_id, radius)) = actions.annulus_create {
            self.solver.doc.add_annulus(center_id, radius * 0.5, radius);
            self.solver.mark_dirty();
            self.status_msg = format!("圆环已创建 (R={radius:.1})");
        }

        // 多边形/贝塞尔完成
        if actions.finish_multi_click {
            match self.tool {
                Tool::Polygon => {
                    if self.interaction.polygon_vertices.len() >= 3 {
                        let verts = std::mem::take(&mut self.interaction.polygon_vertices);
                        self.solver.doc.add_polygon(verts);
                        self.solver.mark_dirty();
                        self.status_msg = "多边形已创建".into();
                    } else {
                        self.status_msg = "多边形至少需要 3 个顶点".into();
                    }
                }
                Tool::Bezier => {
                    let n = self.interaction.bezier_points.len();
                    if n == 3 || n == 4 {
                        let pts = std::mem::take(&mut self.interaction.bezier_points);
                        self.solver.doc.add_bezier(pts);
                        self.solver.mark_dirty();
                        self.status_msg = format!("{}`贝塞尔曲线已创建", if n == 3 { "二阶" } else { "三阶" });
                    } else {
                        self.status_msg = "贝塞尔需要 3 或 4 个控制点".into();
                    }
                }
                _ => {}
            }
        }

        // 角度标注创建
        if actions.angle_mark_create
            && self.interaction.angle_points.len() >= 3 {
                let pts = std::mem::take(&mut self.interaction.angle_points);
                self.solver.doc.add_angle_mark(pts[0], pts[1], pts[2]);
                self.solver.mark_dirty();
                self.status_msg = "角度标注已创建".into();
            }

        // 长度标注创建
        if actions.length_mark_create
            && self.interaction.length_points.len() >= 2
        {
            let pts = std::mem::take(&mut self.interaction.length_points);
            self.solver.doc.add_length_mark(pts[0], pts[1]);
            self.solver.mark_dirty();
            self.status_msg = "长度标注已创建".into();
        }

        // 三角形创建
        if actions.triangle_create
            && self.interaction.triangle_points.len() >= 3
        {
            let pts = std::mem::take(&mut self.interaction.triangle_points);
            self.solver.doc.add_triangle(
                pts[0],
                pts[1],
                pts[2],
                crate::definitions::TriangleType::Scalene,
            );
            self.solver.mark_dirty();
            self.status_msg = "三角形已创建".into();
        }

        // 坐标系网格创建
        if let Some(pos) = actions.grid_create {
            let origin = self.solver.add_free_point(pos);
            self.solver.doc.add_grid(origin, 50.0, true, 5, true);
            self.solver.mark_dirty();
            self.status_msg = "坐标系网格已创建".into();
        }

        if let Some((id, pos)) = actions.drag_point {
            self.solver.update_free_point(id, pos);
        }

        if let Some(id) = actions.delete_element {
            self.solver.remove_element(id);
            self.interaction.selected = None;
            self.status_msg = "已删除".into();
        }

        if actions.clear {
            self.solver = GeometrySolver::new();
            self.interaction = InteractionState::default();
            self.point_counter = 0;
            self.status_msg = "已清空".into();
        }

        if actions.save {
            if let Some(ref path) = self.save_path {
                match persistence::save_to_json(&self.solver.doc, path) {
                    Ok(_) => self.status_msg = format!("已保存到 {path}"),
                    Err(e) => self.status_msg = format!("保存失败: {e}"),
                }
            } else {
                let path = std::env::temp_dir().join("drafftink_geometry.json");
                let path_str = path.to_string_lossy().to_string();
                match persistence::save_to_json(&self.solver.doc, &path) {
                    Ok(_) => {
                        self.status_msg = format!("已保存到 {path_str}");
                        self.save_path = Some(path_str);
                    }
                    Err(e) => self.status_msg = format!("保存失败: {e}"),
                }
            }
        }

        if actions.load {
            let path = std::env::temp_dir().join("drafftink_geometry.json");
            match persistence::load_from_json(&path) {
                Ok(doc) => {
                    self.solver = GeometrySolver::from_doc(doc);
                    self.interaction = InteractionState::default();
                    self.status_msg = "已加载".into();
                }
                Err(e) => self.status_msg = format!("加载失败: {e}"),
            }
        }

        let _ = ctx;
    }

    fn process_actions_3d(&mut self, actions: UiActions3D) {
        if let Some(tool) = actions.tool_3d {
            self.tool_3d = tool;
            self.status_msg = format!("已选择: {}", tool.label());
        }

        if actions.add_primitive {
            self.add_3d_primitive();
        }

        if actions.toggle_projection {
            self.camera_3d.toggle_projection();
            self.status_msg = format!(
                "投影: {}",
                match self.camera_3d.projection {
                    ProjectionMode::Perspective => "透视",
                    ProjectionMode::Orthographic => "正交",
                }
            );
        }

        if actions.reset_camera {
            self.camera_3d.reset();
            self.status_msg = "视角已重置".into();
        }

        if let Some(mode) = actions.render_mode {
            self.render_mode_3d = mode;
            self.status_msg = match mode {
                RenderMode3D::Wireframe => "渲染模式: 线框".into(),
                RenderMode3D::Solid => "渲染模式: 实心".into(),
                RenderMode3D::SolidWireframe => "渲染模式: 实心+线框".into(),
            };
        }

        if actions.back_to_2d {
            self.is_3d = false;
            self.status_msg = "2D 模式".into();
        }

        if actions.toggle_seewo_compat {
            if self.seewo_compat_mode {
                self.status_msg = "希沃兼容模式: 开启（纯黑线框）".into();
            } else {
                self.status_msg = "希沃兼容模式: 关闭（现代实心渲染）".into();
            }
        }

        if actions.import_seewo_xml {
            self.import_seewo_xml_file();
        }
    }

    /// 导入希沃 EasiNote XML 文件
    fn import_seewo_xml_file(&mut self) {
        let dialog = rfd::FileDialog::new()
            .add_filter("希沃课件 XML", &["xml"])
            .set_title("选择希沃 EasiNote Slide XML 文件");

        match dialog.pick_file() {
            Some(path) => {
                match std::fs::read_to_string(&path) {
                    Ok(xml_content) => {
                        match seewo_import::parse_slide_xml(&xml_content) {
                            Ok(slide) => {
                                let n = slide.cylinders.len() + slide.cones.len();
                                self.seewo_slide = Some(slide);
                                self.is_3d = true;
                                self.seewo_compat_mode = false;
                                self.status_msg = format!("已导入 {n} 个希沃 3D 对象");
                            }
                            Err(e) => {
                                self.status_msg = format!("XML 解析失败: {e}");
                            }
                        }
                    }
                    Err(e) => {
                        self.status_msg = format!("文件读取失败: {e}");
                    }
                }
            }
            None => {
                self.status_msg = "已取消导入".into();
            }
        }
    }

    /// 添加 3D 基本体
    fn add_3d_primitive(&mut self) {
        match self.tool_3d {
            Tool3D::Cube => {
                let center = self.solver.add_free_point_3d(Point3D::new(0.0, 0.0, 0.0));
                match self.solver.add_cube(center, 2.0) {
                    Ok(_) => self.status_msg = "立方体已添加".into(),
                    Err(e) => self.status_msg = format!("错误: {e}"),
                }
            }
            Tool3D::Cuboid => {
                let center = self.solver.add_free_point_3d(Point3D::new(0.0, 0.0, 0.0));
                self.solver.doc.add_cuboid(center, 3.0, 2.0, 1.5);
                self.solver.mark_dirty();
                self.status_msg = "长方体已添加".into();
            }
            Tool3D::Prism => {
                let bot = self.solver.add_free_point_3d(Point3D::new(0.0, -1.5, 0.0));
                let top = self.solver.add_free_point_3d(Point3D::new(0.0, 1.5, 0.0));
                self.solver.doc.add_prism(bot, top, 1.5, 6);
                self.solver.mark_dirty();
                self.status_msg = "六棱柱已添加".into();
            }
            Tool3D::Pyramid => {
                let base = self.solver.add_free_point_3d(Point3D::new(0.0, -1.5, 0.0));
                let apex = self.solver.add_free_point_3d(Point3D::new(0.0, 1.5, 0.0));
                self.solver.doc.add_pyramid(base, apex, 1.5, 4);
                self.solver.mark_dirty();
                self.status_msg = "四棱锥已添加".into();
            }
            Tool3D::Sphere => {
                let center = self.solver.add_free_point_3d(Point3D::new(0.0, 0.0, 0.0));
                match self.solver.add_sphere(center, 1.5) {
                    Ok(_) => self.status_msg = "球体已添加".into(),
                    Err(e) => self.status_msg = format!("错误: {e}"),
                }
            }
            Tool3D::Cylinder => {
                let bot = self.solver.add_free_point_3d(Point3D::new(0.0, -1.5, 0.0));
                let top = self.solver.add_free_point_3d(Point3D::new(0.0, 1.5, 0.0));
                self.solver.doc.add_cylinder(bot, top, 1.0);
                self.solver.mark_dirty();
                self.status_msg = "圆柱已添加".into();
            }
            Tool3D::Cone => {
                let base = self.solver.add_free_point_3d(Point3D::new(0.0, -1.5, 0.0));
                let apex = self.solver.add_free_point_3d(Point3D::new(0.0, 1.5, 0.0));
                self.solver.doc.add_cone(base, apex, 1.5);
                self.solver.mark_dirty();
                self.status_msg = "圆锥已添加".into();
            }
            Tool3D::Frustum => {
                let bot = self.solver.add_free_point_3d(Point3D::new(0.0, -1.5, 0.0));
                let top = self.solver.add_free_point_3d(Point3D::new(0.0, 1.5, 0.0));
                self.solver.doc.add_frustum(bot, top, 1.5, 0.8);
                self.solver.mark_dirty();
                self.status_msg = "圆台已添加".into();
            }
            Tool3D::Tetrahedron => {
                let center = self.solver.add_free_point_3d(Point3D::new(0.0, 0.0, 0.0));
                self.solver.doc.add_regular_polyhedron(center, 2.0, PolyhedronType::Tetrahedron);
                self.solver.mark_dirty();
                self.status_msg = "正四面体已添加".into();
            }
            Tool3D::Octahedron => {
                let center = self.solver.add_free_point_3d(Point3D::new(0.0, 0.0, 0.0));
                self.solver.doc.add_regular_polyhedron(center, 2.0, PolyhedronType::Octahedron);
                self.solver.mark_dirty();
                self.status_msg = "正八面体已添加".into();
            }
        }
    }

    fn render_close_button(&self, ctx: &egui::Context) {
        let _ = ctx;
    }
}

// ── UI 辅助 ─────────────────────────────────────────────────────

/// 创建分节标签
fn section_label(text: &str) -> egui::RichText {
    egui::RichText::new(text)
        .size(12.0)
        .color(Color32::from_rgb(150, 160, 180))
}

/// 渲染 2D 工具按钮组（泛型，支持任意数量）
fn tool_buttons<const N: usize>(
    ui: &mut egui::Ui,
    current: Tool,
    tools: [(Tool, &str); N],
    actions: &mut UiActions,
) {
    ui.horizontal_wrapped(|ui| {
        for (tool, label) in tools {
            let selected = current == tool;
            let btn = if selected {
                egui::Button::new(label).fill(Color32::from_rgb(60, 100, 160))
            } else {
                egui::Button::new(label)
            };
            if ui.add(btn).clicked() {
                actions.tool_changed = Some(tool);
            }
        }
    });
}

/// 渲染 3D 工具按钮组（泛型，支持任意数量）
fn tool3d_buttons<const N: usize>(
    ui: &mut egui::Ui,
    current: Tool3D,
    tools: [(Tool3D, &str); N],
    actions: &mut UiActions3D,
) {
    ui.horizontal_wrapped(|ui| {
        for (tool, label) in tools {
            let selected = current == tool;
            let btn = if selected {
                egui::Button::new(label).fill(Color32::from_rgb(60, 100, 160))
            } else {
                egui::Button::new(label)
            };
            if ui.add(btn).clicked() {
                actions.tool_3d = Some(tool);
            }
        }
    });
}

// ── UI 动作收集 ─────────────────────────────────────────────────

#[derive(Default)]
struct UiActions {
    tool_changed: Option<Tool>,
    toggle_3d: bool,
    add_point: Option<Point2D>,
    line_click: Option<Uuid>,
    circle_create: Option<(Uuid, f32)>,
    regular_polygon_create: Option<(Uuid, f32)>,
    arc_create: Option<(Uuid, f32)>,
    sector_create: Option<(Uuid, f32)>,
    ellipse_create: Option<(Uuid, f32)>,
    annulus_create: Option<(Uuid, f32)>,
    finish_multi_click: bool,
    angle_mark_create: bool,
    length_mark_create: bool,
    triangle_create: bool,
    grid_create: Option<Point2D>,
    drag_point: Option<(Uuid, Point2D)>,
    delete_element: Option<Uuid>,
    clear: bool,
    save: bool,
    load: bool,
}

#[derive(Default)]
struct UiActions3D {
    tool_3d: Option<Tool3D>,
    add_primitive: bool,
    toggle_projection: bool,
    reset_camera: bool,
    back_to_2d: bool,
    render_mode: Option<RenderMode3D>,
    toggle_seewo_compat: bool,
    import_seewo_xml: bool,
}
