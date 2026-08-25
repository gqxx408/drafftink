//! 网格化层 — lyon 三角化
//!
//! 将几何定义转换为顶点/索引缓冲区，用于 GPU 渲染。
//! 使用 lyon 的 FillTessellator（填充）和 StrokeTessellator（描边）。
//!
//! # 渲染管线
//! 1. solver.solve() → SolverContext（具体坐标）
//! 2. mesh::triangulate_*() → GeometryMesh（顶点 + 索引）
//! 3. renderer::draw() → egui::Painter（GPU 光栅化）
//!
//! 圆形使用 64 段多边形近似，tolerance = 0.01 确保平滑边缘。

use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{Color32, Painter, Pos2, Shape};
use lyon::math::Point as LyonPoint;
use lyon::path::Path as LyonPath;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, LineCap, LineJoin,
    StrokeOptions, StrokeTessellator, StrokeVertex, VertexBuffers,
};
use uuid::Uuid;

use crate::definitions::Point2D;
use crate::solver::SolverContext;

/// 圆形近似段数
pub const CIRCLE_SEGMENTS: usize = 64;

/// 几何网格 — 顶点 + 索引缓冲
#[derive(Debug, Clone, Default)]
pub struct GeometryMesh {
    /// 顶点坐标 [x, y]
    pub vertices: Vec<[f32; 2]>,
    /// 索引缓冲
    pub indices: Vec<u32>,
}

impl GeometryMesh {
    /// 创建空网格
    pub fn new() -> Self {
        Self::default()
    }

    /// 转换为 egui Mesh 并添加到 Painter
    pub fn add_to_painter(&self, painter: &Painter, color: Color32) {
        if self.vertices.is_empty() || self.indices.is_empty() {
            return;
        }

        let mut mesh = Mesh::default();
        mesh.reserve_vertices(self.vertices.len());
        mesh.indices.reserve(self.indices.len());

        for v in &self.vertices {
            mesh.vertices.push(Vertex {
                pos: Pos2::new(v[0], v[1]),
                uv: WHITE_UV,
                color,
            });
        }
        for &idx in &self.indices {
            mesh.indices.push(idx);
        }

        painter.add(Shape::mesh(mesh));
    }
}

// ── 三角化函数 ──────────────────────────────────────────────────

/// 三角化线段 — 使用 lyon StrokeTessellator 生成粗线条
///
/// 生成一个以 (start, end) 为端点的粗线条三角网格。
pub fn triangulate_line(start: Point2D, end: Point2D, width: f32) -> GeometryMesh {
    let mut builder = LyonPath::builder();
    builder.begin(LyonPoint::new(start.x, start.y));
    builder.line_to(LyonPoint::new(end.x, end.y));
    builder.end(false);
    let path = builder.build();

    let mut buffers: VertexBuffers<LyonPoint, u32> = VertexBuffers::new();
    let stroke_opts = StrokeOptions::default()
        .with_line_width(width)
        .with_tolerance(0.01)
        .with_line_cap(LineCap::Round)
        .with_line_join(LineJoin::Round);

    let mut tess = StrokeTessellator::new();
    let mut buffers_builder = BuffersBuilder::new(&mut buffers, |v: StrokeVertex| v.position());

    if tess
        .tessellate_path(&path, &stroke_opts, &mut buffers_builder)
        .is_err()
    {
        log::warn!("线段三角化失败");
        return GeometryMesh::new();
    }

    buffers_to_mesh(&buffers)
}

/// 三角化圆形 — 使用 lyon FillTessellator 填充
///
/// 使用 64 段多边形近似圆，tolerance = 0.01 确保平滑边缘。
pub fn triangulate_circle(center: Point2D, radius: f32) -> GeometryMesh {
    if radius <= 0.0 {
        return GeometryMesh::new();
    }

    let mut builder = LyonPath::builder();

    // 构建 64 段多边形近似圆
    for i in 0..=CIRCLE_SEGMENTS {
        let angle = i as f32 / CIRCLE_SEGMENTS as f32 * std::f32::consts::TAU;
        let x = center.x + radius * angle.cos();
        let y = center.y + radius * angle.sin();
        if i == 0 {
            builder.begin(LyonPoint::new(x, y));
        } else {
            builder.line_to(LyonPoint::new(x, y));
        }
    }
    builder.end(true);
    let path = builder.build();

    let mut buffers: VertexBuffers<LyonPoint, u32> = VertexBuffers::new();
    let fill_options = FillOptions::default()
        .with_tolerance(0.01)
        .with_fill_rule(FillRule::NonZero);

    let mut tess = FillTessellator::new();
    let mut buffers_builder = BuffersBuilder::new(&mut buffers, |v: FillVertex| v.position());

    if tess
        .tessellate_path(&path, &fill_options, &mut buffers_builder)
        .is_err()
    {
        log::warn!("圆形三角化失败");
        return GeometryMesh::new();
    }

    buffers_to_mesh(&buffers)
}

/// 三角化圆形描边 — 使用 lyon StrokeTessellator
///
/// 生成圆环（空心圆），用于圆的轮廓渲染。
pub fn triangulate_circle_stroke(center: Point2D, radius: f32, width: f32) -> GeometryMesh {
    if radius <= 0.0 {
        return GeometryMesh::new();
    }

    let mut builder = LyonPath::builder();

    for i in 0..=CIRCLE_SEGMENTS {
        let angle = i as f32 / CIRCLE_SEGMENTS as f32 * std::f32::consts::TAU;
        let x = center.x + radius * angle.cos();
        let y = center.y + radius * angle.sin();
        if i == 0 {
            builder.begin(LyonPoint::new(x, y));
        } else {
            builder.line_to(LyonPoint::new(x, y));
        }
    }
    builder.end(true);
    let path = builder.build();

    let mut buffers: VertexBuffers<LyonPoint, u32> = VertexBuffers::new();
    let stroke_opts = StrokeOptions::default()
        .with_line_width(width)
        .with_tolerance(0.01)
        .with_line_cap(LineCap::Round)
        .with_line_join(LineJoin::Round);

    let mut tess = StrokeTessellator::new();
    let mut buffers_builder = BuffersBuilder::new(&mut buffers, |v: StrokeVertex| v.position());

    if tess
        .tessellate_path(&path, &stroke_opts, &mut buffers_builder)
        .is_err()
    {
        log::warn!("圆形描边三角化失败");
        return GeometryMesh::new();
    }

    buffers_to_mesh(&buffers)
}

/// 三角化点 — 小填充圆
///
/// 点渲染为半径 `size` 的小填充圆。
pub fn triangulate_point(pos: Point2D, size: f32) -> GeometryMesh {
    triangulate_circle(pos, size)
}

/// 三角化多边形描边 — 使用 lyon StrokeTessellator
///
/// 构建闭合路径并以给定宽度描边。
pub fn triangulate_polygon_stroke(points: &[Point2D], width: f32) -> GeometryMesh {
    if points.len() < 2 {
        return GeometryMesh::new();
    }

    let mut builder = LyonPath::builder();
    builder.begin(LyonPoint::new(points[0].x, points[0].y));
    for p in &points[1..] {
        builder.line_to(LyonPoint::new(p.x, p.y));
    }
    builder.end(true); // 闭合
    let path = builder.build();

    let mut buffers: VertexBuffers<LyonPoint, u32> = VertexBuffers::new();
    let stroke_opts = StrokeOptions::default()
        .with_line_width(width)
        .with_tolerance(0.01)
        .with_line_cap(LineCap::Round)
        .with_line_join(LineJoin::Round);

    let mut tess = StrokeTessellator::new();
    let mut buffers_builder = BuffersBuilder::new(&mut buffers, |v: StrokeVertex| v.position());

    if tess
        .tessellate_path(&path, &stroke_opts, &mut buffers_builder)
        .is_err()
    {
        log::warn!("多边形描边三角化失败");
        return GeometryMesh::new();
    }

    buffers_to_mesh(&buffers)
}

/// 三角化多边形填充 — 使用 lyon FillTessellator
///
/// 构建闭合路径并填充，使用 NonZero 填充规则。
pub fn triangulate_polygon_fill(points: &[Point2D]) -> GeometryMesh {
    if points.len() < 3 {
        return GeometryMesh::new();
    }

    let mut builder = LyonPath::builder();
    builder.begin(LyonPoint::new(points[0].x, points[0].y));
    for p in &points[1..] {
        builder.line_to(LyonPoint::new(p.x, p.y));
    }
    builder.end(true); // 闭合
    let path = builder.build();

    let mut buffers: VertexBuffers<LyonPoint, u32> = VertexBuffers::new();
    let fill_options = FillOptions::default()
        .with_tolerance(0.01)
        .with_fill_rule(FillRule::NonZero);

    let mut tess = FillTessellator::new();
    let mut buffers_builder = BuffersBuilder::new(&mut buffers, |v: FillVertex| v.position());

    if tess
        .tessellate_path(&path, &fill_options, &mut buffers_builder)
        .is_err()
    {
        log::warn!("多边形填充三角化失败");
        return GeometryMesh::new();
    }

    buffers_to_mesh(&buffers)
}

/// 三角化圆弧 — 使用 lyon StrokeTessellator
///
/// 以 64 段近似圆弧并描边。
/// 处理角度环绕：若 end_angle < start_angle，则加上 TAU。
pub fn triangulate_arc(
    center: Point2D,
    radius: f32,
    start_angle: f32,
    end_angle: f32,
    width: f32,
) -> GeometryMesh {
    if radius <= 0.0 {
        return GeometryMesh::new();
    }

    let start = start_angle;
    let mut end = end_angle;
    if end < start {
        end += std::f32::consts::TAU;
    }
    // 若起止相同，退化为空
    if (end - start).abs() < 1e-6 {
        return GeometryMesh::new();
    }

    let mut builder = LyonPath::builder();
    for i in 0..=CIRCLE_SEGMENTS {
        let t = i as f32 / CIRCLE_SEGMENTS as f32;
        let angle = start + (end - start) * t;
        let x = center.x + radius * angle.cos();
        let y = center.y + radius * angle.sin();
        if i == 0 {
            builder.begin(LyonPoint::new(x, y));
        } else {
            builder.line_to(LyonPoint::new(x, y));
        }
    }
    builder.end(false); // 开放路径
    let path = builder.build();

    let mut buffers: VertexBuffers<LyonPoint, u32> = VertexBuffers::new();
    let stroke_opts = StrokeOptions::default()
        .with_line_width(width)
        .with_tolerance(0.01)
        .with_line_cap(LineCap::Round)
        .with_line_join(LineJoin::Round);

    let mut tess = StrokeTessellator::new();
    let mut buffers_builder = BuffersBuilder::new(&mut buffers, |v: StrokeVertex| v.position());

    if tess
        .tessellate_path(&path, &stroke_opts, &mut buffers_builder)
        .is_err()
    {
        log::warn!("圆弧三角化失败");
        return GeometryMesh::new();
    }

    buffers_to_mesh(&buffers)
}

/// 三角化扇形 — 使用 lyon FillTessellator
///
/// 构建闭合路径：中心 → 弧起点 → 弧上各点 → 回到中心，然后填充。
pub fn triangulate_sector(
    center: Point2D,
    radius: f32,
    start_angle: f32,
    end_angle: f32,
) -> GeometryMesh {
    if radius <= 0.0 {
        return GeometryMesh::new();
    }

    let start = start_angle;
    let mut end = end_angle;
    if end < start {
        end += std::f32::consts::TAU;
    }
    if (end - start).abs() < 1e-6 {
        return GeometryMesh::new();
    }

    let mut builder = LyonPath::builder();
    // 中心点开始
    builder.begin(LyonPoint::new(center.x, center.y));
    for i in 0..=CIRCLE_SEGMENTS {
        let t = i as f32 / CIRCLE_SEGMENTS as f32;
        let angle = start + (end - start) * t;
        let x = center.x + radius * angle.cos();
        let y = center.y + radius * angle.sin();
        builder.line_to(LyonPoint::new(x, y));
    }
    // 回到中心
    builder.line_to(LyonPoint::new(center.x, center.y));
    builder.end(true); // 闭合
    let path = builder.build();

    let mut buffers: VertexBuffers<LyonPoint, u32> = VertexBuffers::new();
    let fill_options = FillOptions::default()
        .with_tolerance(0.01)
        .with_fill_rule(FillRule::NonZero);

    let mut tess = FillTessellator::new();
    let mut buffers_builder = BuffersBuilder::new(&mut buffers, |v: FillVertex| v.position());

    if tess
        .tessellate_path(&path, &fill_options, &mut buffers_builder)
        .is_err()
    {
        log::warn!("扇形三角化失败");
        return GeometryMesh::new();
    }

    buffers_to_mesh(&buffers)
}

/// 三角化椭圆描边 — 使用 lyon StrokeTessellator
///
/// 以 64 段近似椭圆并描边，支持旋转。
/// 每段角度 θ：
///   x = center.x + semi_a * cos(θ) * cos(rotation) - semi_b * sin(θ) * sin(rotation)
///   y = center.y + semi_a * cos(θ) * sin(rotation) + semi_b * sin(θ) * cos(rotation)
pub fn triangulate_ellipse_stroke(
    center: Point2D,
    semi_a: f32,
    semi_b: f32,
    rotation: f32,
    width: f32,
) -> GeometryMesh {
    if semi_a <= 0.0 || semi_b <= 0.0 {
        return GeometryMesh::new();
    }

    let cos_r = rotation.cos();
    let sin_r = rotation.sin();

    let mut builder = LyonPath::builder();
    for i in 0..=CIRCLE_SEGMENTS {
        let theta = i as f32 / CIRCLE_SEGMENTS as f32 * std::f32::consts::TAU;
        let cos_t = theta.cos();
        let sin_t = theta.sin();
        let x = center.x + semi_a * cos_t * cos_r - semi_b * sin_t * sin_r;
        let y = center.y + semi_a * cos_t * sin_r + semi_b * sin_t * cos_r;
        if i == 0 {
            builder.begin(LyonPoint::new(x, y));
        } else {
            builder.line_to(LyonPoint::new(x, y));
        }
    }
    builder.end(true); // 闭合
    let path = builder.build();

    let mut buffers: VertexBuffers<LyonPoint, u32> = VertexBuffers::new();
    let stroke_opts = StrokeOptions::default()
        .with_line_width(width)
        .with_tolerance(0.01)
        .with_line_cap(LineCap::Round)
        .with_line_join(LineJoin::Round);

    let mut tess = StrokeTessellator::new();
    let mut buffers_builder = BuffersBuilder::new(&mut buffers, |v: StrokeVertex| v.position());

    if tess
        .tessellate_path(&path, &stroke_opts, &mut buffers_builder)
        .is_err()
    {
        log::warn!("椭圆描边三角化失败");
        return GeometryMesh::new();
    }

    buffers_to_mesh(&buffers)
}

/// 三角化圆环 — 使用 lyon FillTessellator + EvenOdd 填充规则
///
/// 构建外圆（顺时针）后通过 move_to 切换到内圆（逆时针），以 EvenOdd 规则填充形成圆环。
pub fn triangulate_annulus(center: Point2D, inner_radius: f32, outer_radius: f32) -> GeometryMesh {
    if inner_radius <= 0.0 || outer_radius <= 0.0 || inner_radius >= outer_radius {
        return GeometryMesh::new();
    }

    let mut builder = LyonPath::builder();

    // 外圆 — 顺时针
    for i in 0..=CIRCLE_SEGMENTS {
        let angle = i as f32 / CIRCLE_SEGMENTS as f32 * std::f32::consts::TAU;
        let x = center.x + outer_radius * angle.cos();
        let y = center.y + outer_radius * angle.sin();
        if i == 0 {
            builder.begin(LyonPoint::new(x, y));
        } else {
            builder.line_to(LyonPoint::new(x, y));
        }
    }
    builder.end(true); // 闭合外圆

    // 内圆 — 逆时针（通过 begin 再次开始一个子路径）
    for i in 0..=CIRCLE_SEGMENTS {
        // 逆时针：使用 -angle
        let angle = -(i as f32 / CIRCLE_SEGMENTS as f32 * std::f32::consts::TAU);
        let x = center.x + inner_radius * angle.cos();
        let y = center.y + inner_radius * angle.sin();
        if i == 0 {
            builder.begin(LyonPoint::new(x, y));
        } else {
            builder.line_to(LyonPoint::new(x, y));
        }
    }
    builder.end(true); // 闭合内圆

    let path = builder.build();

    let mut buffers: VertexBuffers<LyonPoint, u32> = VertexBuffers::new();
    let fill_options = FillOptions::default()
        .with_tolerance(0.01)
        .with_fill_rule(FillRule::EvenOdd);

    let mut tess = FillTessellator::new();
    let mut buffers_builder = BuffersBuilder::new(&mut buffers, |v: FillVertex| v.position());

    if tess
        .tessellate_path(&path, &fill_options, &mut buffers_builder)
        .is_err()
    {
        log::warn!("圆环三角化失败");
        return GeometryMesh::new();
    }

    buffers_to_mesh(&buffers)
}

// ── 批量三角化 ──────────────────────────────────────────────────

/// 可渲染的几何元素（用于批量三角化）
#[derive(Debug, Clone)]
pub enum GeometryElement {
    /// 点
    Point { id: Uuid, size: f32 },
    /// 线段
    Line { id: Uuid, width: f32 },
    /// 圆
    Circle { id: Uuid, stroke_width: f32 },
}

/// 批量三角化所有元素
///
/// 遍历 solver 求解结果，将每个元素转换为 GeometryMesh。
pub fn triangulate_all(
    ctx: &SolverContext,
    point_ids: &[Uuid],
    line_ids: &[(Uuid, Uuid, Uuid, f32)], // (line_id, start_id, end_id, width)
    circle_ids: &[(Uuid, Uuid, f32, f32)], // (circle_id, center_id, radius, stroke_width)
    point_size: f32,
) -> Vec<(GeometryElement, GeometryMesh)> {
    let mut result = Vec::new();

    // 三角化点
    for &id in point_ids {
        if let Some(pos) = ctx.get_2d(id) {
            let mesh = triangulate_point(pos, point_size);
            result.push((
                GeometryElement::Point {
                    id,
                    size: point_size,
                },
                mesh,
            ));
        }
    }

    // 三角化线
    for &(line_id, start_id, end_id, width) in line_ids {
        if let (Some(start), Some(end)) = (ctx.get_2d(start_id), ctx.get_2d(end_id)) {
            let mesh = triangulate_line(start, end, width);
            result.push((GeometryElement::Line { id: line_id, width }, mesh));
        }
    }

    // 三角化圆
    for &(circle_id, center_id, radius, stroke_width) in circle_ids {
        if let Some(center) = ctx.get_2d(center_id) {
            let mesh = triangulate_circle_stroke(center, radius, stroke_width);
            result.push((
                GeometryElement::Circle {
                    id: circle_id,
                    stroke_width,
                },
                mesh,
            ));
        }
    }

    result
}

// ── 辅助函数 ────────────────────────────────────────────────────

/// 将 lyon VertexBuffers 转换为 GeometryMesh
fn buffers_to_mesh(buffers: &VertexBuffers<LyonPoint, u32>) -> GeometryMesh {
    GeometryMesh {
        vertices: buffers.vertices.iter().map(|p| [p.x, p.y]).collect(),
        indices: buffers.indices.clone(),
    }
}

// ── 测试 ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_triangulate_line() {
        let mesh = triangulate_line(Point2D::new(0.0, 0.0), Point2D::new(100.0, 0.0), 4.0);
        // 粗线条应生成至少 4 个顶点（两端各 2 个）
        assert!(mesh.vertices.len() >= 4);
        assert!(!mesh.indices.is_empty());
    }

    #[test]
    fn test_triangulate_circle() {
        let mesh = triangulate_circle(Point2D::new(50.0, 50.0), 30.0);
        // 64 段多边形填充应生成多个顶点
        assert!(mesh.vertices.len() >= 64);
        assert!(!mesh.indices.is_empty());
    }

    #[test]
    fn test_triangulate_circle_stroke() {
        let mesh = triangulate_circle_stroke(Point2D::new(0.0, 0.0), 50.0, 2.0);
        assert!(mesh.vertices.len() >= 64);
        assert!(!mesh.indices.is_empty());
    }

    #[test]
    fn test_triangulate_point() {
        let mesh = triangulate_point(Point2D::new(10.0, 10.0), 5.0);
        // 点是小圆，应有顶点
        assert!(!mesh.vertices.is_empty());
    }

    #[test]
    fn test_zero_radius_circle() {
        let mesh = triangulate_circle(Point2D::new(0.0, 0.0), 0.0);
        assert!(mesh.vertices.is_empty());
    }

    #[test]
    fn test_empty_mesh() {
        let mesh = GeometryMesh::new();
        assert!(mesh.vertices.is_empty());
        assert!(mesh.indices.is_empty());
    }

    #[test]
    fn test_triangulate_polygon_stroke() {
        let pts = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(100.0, 0.0),
            Point2D::new(50.0, 80.0),
        ];
        let mesh = triangulate_polygon_stroke(&pts, 2.0);
        assert!(!mesh.vertices.is_empty());
    }

    #[test]
    fn test_triangulate_arc() {
        let mesh = triangulate_arc(
            Point2D::new(50.0, 50.0),
            30.0,
            0.0,
            std::f32::consts::FRAC_PI_2,
            2.0,
        );
        assert!(!mesh.vertices.is_empty());
    }

    #[test]
    fn test_triangulate_ellipse_stroke() {
        let mesh = triangulate_ellipse_stroke(Point2D::new(50.0, 50.0), 40.0, 20.0, 0.0, 2.0);
        assert!(!mesh.vertices.is_empty());
    }
}
