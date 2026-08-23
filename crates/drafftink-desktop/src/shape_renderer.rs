//! 9 种形状的纯 egui Painter 绘制实现（零额外依赖）。
//!
//! 形状作为宿主层叠加层渲染（与视频/图片叠加层同构），不在文档层持久化。
//! 绘制完全依赖 `egui::Painter`，不使用任何字体图标或外部 crate。
//!
//! 设计要点：
//! - `draw_shape` 是统一入口，按 [`ShapeKind`] 分派到具体绘制函数。
//! - 括号 / 箭头等曲线形状拆出**纯几何 helper**（`parenthesis_points` /
//!   `bracket_segments` / `brace_beziers` / `arrow_head`），既供绘制调用，
//!   也可被单元测试直接验证（无需 `Painter` 上下文）。
//! - 与视频/图片叠加层复用同一套 `RectInteraction`（`interactive_rect.rs`）做
//!   8 方向缩放与拖拽移动，本模块只负责「画」、不负责「交互」。

use drafftink_core::model::ShapeKind;
use egui::epaint::CubicBezierShape;
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};

/// 圆角矩形的圆角半径（px，屏幕空间）。
const ROUNDING: f32 = 12.0;

/// 正多边形默认起始角（度）：90° 使第一个顶点朝上，三角形底边水平、顶点朝上。
pub const POLYGON_DEFAULT_START_DEG: f32 = 90.0;

/// 在 `rect` 内绘制指定种类的形状。
///
/// - `stroke`：描边线型（线宽 + 颜色）。
/// - `fill`：填充颜色；`None` 表示仅描边不填充。
/// - `arc_degrees`：弧 / 扇 / 角的起止角（度，屏幕空间，0°=正右、逆时针为正）。
/// - `line_flipped`：线段方向（`Line` 用）。
pub fn draw_shape(
    painter: &Painter,
    rect: Rect,
    kind: ShapeKind,
    stroke: Stroke,
    fill: Option<Color32>,
    arc_degrees: Option<(f32, f32)>,
    line_flipped: bool,
) {
    match kind {
        ShapeKind::Circle => {
            // 外接圆：取宽高中较小者为直径，圆心为 rect 中心。
            let radius = rect.width().min(rect.height()) / 2.0;
            let center = rect.center();
            if let Some(f) = fill {
                painter.circle_filled(center, radius, f);
            }
            painter.circle_stroke(center, radius, stroke);
        }
        ShapeKind::Square | ShapeKind::Rectangle => {
            // Square 与 Rectangle 渲染无差别——比例完全由 rect 决定。
            if let Some(f) = fill {
                painter.rect_filled(rect, 0.0, f);
            }
            painter.rect_stroke(rect, 0.0, stroke);
        }
        ShapeKind::RoundedRect => {
            if let Some(f) = fill {
                painter.rect_filled(rect, ROUNDING, f);
            }
            painter.rect_stroke(rect, ROUNDING, stroke);
        }
        ShapeKind::Parenthesis => {
            painter.add(Shape::line(parenthesis_points(rect), stroke));
        }
        ShapeKind::Bracket => {
            for (a, b) in bracket_segments(rect) {
                painter.line_segment([a, b], stroke);
            }
        }
        ShapeKind::Brace => {
            for b in brace_beziers(rect) {
                painter.add(Shape::CubicBezier(CubicBezierShape {
                    points: b,
                    closed: false,
                    fill: Color32::TRANSPARENT,
                    stroke: stroke.into(),
                }));
            }
        }
        ShapeKind::Arrow => draw_arrow(painter, rect, stroke, fill, false),
        ShapeKind::DoubleArrow => draw_arrow(painter, rect, stroke, fill, true),
        // ── 虚拟教具产物（Line/Arc/Sector/Angle）──────────────────────────
        ShapeKind::Line => {
            // 线段 = rect 对角线；方向由 `line_flipped` 决定。
            let (a, b) = if line_flipped {
                (rect.right_top(), rect.left_bottom())
            } else {
                (rect.left_top(), rect.right_bottom())
            };
            painter.line_segment([a, b], stroke);
        }
        ShapeKind::Arc => {
            let center = rect.center();
            let radius = rect.width().min(rect.height()) / 2.0;
            let (start, end) = arc_degrees.unwrap_or((0.0, 90.0));
            for (p0, p1) in arc_segments(center, radius, start, end, 64) {
                painter.line_segment([p0, p1], stroke);
            }
        }
        ShapeKind::Sector => {
            let center = rect.center();
            let radius = rect.width().min(rect.height()) / 2.0;
            let (start, end) = arc_degrees.unwrap_or((0.0, 90.0));
            // 扇形填充（≤180° 为凸多边形：圆心 + 弧上采样点）。
            if let Some(f) = fill {
                let pts = sector_points(center, radius, start, end, 64);
                painter.add(Shape::convex_polygon(pts, f, Stroke::NONE));
            }
            // 弧 + 两条半径。
            for (p0, p1) in arc_segments(center, radius, start, end, 64) {
                painter.line_segment([p0, p1], stroke);
            }
            painter.line_segment([center, angle_point(center, radius, start)], stroke);
            painter.line_segment([center, angle_point(center, radius, end)], stroke);
        }
        ShapeKind::Angle => {
            let center = rect.center();
            let radius = rect.width().min(rect.height()) / 2.0;
            let (a0, a1) = arc_degrees.unwrap_or((0.0, 45.0));
            painter.line_segment([center, angle_point(center, radius, a0)], stroke);
            painter.line_segment([center, angle_point(center, radius, a1)], stroke);
        }
        ShapeKind::Polygon { sides, .. } => {
            // 与 Arc/Sector 一致：中心/半径由 rect（叠加层屏幕矩形，随拖拽缩放变化）
            // 派生，保证提交后几何仍跟随选中框；sides 决定顶点数。
            let center = rect.center();
            let radius = rect.width().min(rect.height()) / 2.0;
            let pts = polygon_vertices(center, radius, sides, POLYGON_DEFAULT_START_DEG);
            if let Some(f) = fill {
                painter.add(Shape::convex_polygon(pts.clone(), f, Stroke::NONE));
            }
            painter.add(Shape::closed_line(pts, stroke));
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 教具几何 helper（纯函数，可单测）
// ─────────────────────────────────────────────────────────────────────────────

/// 角度（度，屏幕空间）→ 单位方向向量（0°=正右，逆时针为正；屏幕 y 向下故 sin 取负）。
pub fn angle_vec(deg: f32) -> Vec2 {
    let rad = deg.to_radians();
    Vec2::new(rad.cos(), -rad.sin())
}

/// 圆心 + 半径 + 角度（度）→ 圆上点（屏幕空间）。
pub fn angle_point(center: Pos2, radius: f32, deg: f32) -> Pos2 {
    center + angle_vec(deg) * radius
}

/// 圆弧的采样线段（供 Arc/Sector 描边）：返回 `(p0, p1)` 序列。
pub fn arc_segments(center: Pos2, radius: f32, start_deg: f32, end_deg: f32, steps: usize) -> Vec<(Pos2, Pos2)> {
    let mut segs = Vec::with_capacity(steps);
    for i in 0..steps {
        let a0 = start_deg + (end_deg - start_deg) * (i as f32 / steps as f32);
        let a1 = start_deg + (end_deg - start_deg) * ((i + 1) as f32 / steps as f32);
        segs.push((angle_point(center, radius, a0), angle_point(center, radius, a1)));
    }
    segs
}

/// 扇形顶点序列（圆心 + 弧上采样点，凸多边形，供填充）。
pub fn sector_points(center: Pos2, radius: f32, start_deg: f32, end_deg: f32, steps: usize) -> Vec<Pos2> {
    let mut pts = Vec::with_capacity(steps + 2);
    pts.push(center);
    for i in 0..=steps {
        let a = start_deg + (end_deg - start_deg) * (i as f32 / steps as f32);
        pts.push(angle_point(center, radius, a));
    }
    pts
}

/// 正多边形的 `sides` 个顶点（屏幕空间，0°=正右、逆时针为正）。
///
/// `start_deg` 为第一个顶点的角度；`i` 号顶点角度 = `start_deg + i * 360/sides`。
/// 供预览（`start_deg = preview_angle`）与提交后渲染（`start_deg` 默认 90°）复用。
pub fn polygon_vertices(center: Pos2, radius: f32, sides: u8, start_deg: f32) -> Vec<Pos2> {
    (0..sides)
        .map(|i| {
            let a = start_deg + i as f32 * 360.0 / sides as f32;
            angle_point(center, radius, a)
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// 几何 helper（纯函数，可单测）
// ─────────────────────────────────────────────────────────────────────────────

/// 小括号 `(` 的采样点（开口朝右）。
///
/// 用椭圆弧近似：角度从 ~36° 扫到 ~324°（288° 跨度），缺口在右侧，形成
/// 形似 `(` 的曲线。x 以 `width*0.3` 为振幅、y 以 `height*0.5` 为振幅。
pub fn parenthesis_points(rect: Rect) -> Vec<Pos2> {
    let center = rect.center();
    let steps = 24;
    let mut pts = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        // π * (0.2 + 0.6t) → 36° .. 324°
        let angle = std::f32::consts::PI * (0.2 + 0.6 * t);
        let x = center.x + rect.width() * 0.30 * angle.cos();
        let y = center.y + rect.height() * 0.5 * angle.sin();
        pts.push(Pos2::new(x, y));
    }
    pts
}

/// 中括号 `[` 的三段线段：左侧竖线 + 上/下横勾（向右）。
pub fn bracket_segments(rect: Rect) -> Vec<(Pos2, Pos2)> {
    let top = rect.top();
    let bottom = rect.bottom();
    let left = rect.left();
    let right = rect.right();
    let tick = (right - left).max(6.0);
    vec![
        (Pos2::new(left, top), Pos2::new(left, bottom)),
        (Pos2::new(left, top), Pos2::new(left + tick, top)),
        (Pos2::new(left, bottom), Pos2::new(left + tick, bottom)),
    ]
}

/// 大括号 `{` 的四段三次贝塞尔控制点（开口朝右）。
///
/// 每段为 `[p0, p1, p2, p3]`；`scale_y` 固定取 0.6 比例（形状叠加层不携带该字段）。
pub fn brace_beziers(rect: Rect) -> Vec<[Pos2; 4]> {
    let x = rect.left();
    let x_mid = rect.right().max(x + 4.0);
    let top = rect.top();
    let bottom = rect.bottom();
    let mid_y = (top + bottom) * 0.5;
    let h = (bottom - top).max(2.0);
    let curv = (x_mid - x) * 0.6;
    let q1 = top + h * 0.25;
    let q3 = top + h * 0.75;
    let cusp = Pos2::new(x_mid, mid_y);
    vec![
        [
            Pos2::new(x, top),
            Pos2::new(x + curv, top),
            Pos2::new(x + curv, q1 - h * 0.05),
            Pos2::new(x_mid - curv * 0.3, q1),
        ],
        [
            Pos2::new(x_mid - curv * 0.3, q1),
            Pos2::new(x_mid, q1 + h * 0.05),
            Pos2::new(x_mid, mid_y - h * 0.08),
            cusp,
        ],
        [
            cusp,
            Pos2::new(x_mid, mid_y + h * 0.08),
            Pos2::new(x_mid, q3 - h * 0.05),
            Pos2::new(x_mid - curv * 0.3, q3),
        ],
        [
            Pos2::new(x_mid - curv * 0.3, q3),
            Pos2::new(x + curv, q3 + h * 0.05),
            Pos2::new(x + curv, bottom),
            Pos2::new(x, bottom),
        ],
    ]
}

/// 箭头头部三角形（以 `to` 为尖端，底边垂直于 `from→to`）。
pub fn arrow_head(from: Pos2, to: Pos2, width: f32) -> [Pos2; 3] {
    let dir = (to - from).normalized();
    let perp = Vec2::new(-dir.y, dir.x);
    let head = width.max(6.0);
    [
        to,
        to - dir * head + perp * head * 0.5,
        to - dir * head - perp * head * 0.5,
    ]
}

// ─────────────────────────────────────────────────────────────────────────────
// 箭头绘制
// ─────────────────────────────────────────────────────────────────────────────

/// 直线 + 箭头头部。
///
/// - `double = false`：仅右端 `→`。
/// - `double = true`：两端各一个箭头 `⇌`。
fn draw_arrow(
    painter: &Painter,
    rect: Rect,
    stroke: Stroke,
    fill: Option<Color32>,
    double: bool,
) {
    let start = Pos2::new(rect.left(), rect.center().y);
    let end = Pos2::new(rect.right(), rect.center().y);
    painter.line_segment([start, end], stroke);

    let head_len = (rect.width().min(rect.height()) * 0.3).max(8.0);

    // 右端箭头头
    let tri = arrow_head(start, end, head_len);
    if let Some(f) = fill {
        painter.add(Shape::convex_polygon(tri.to_vec(), f, Stroke::NONE));
    }
    painter.add(Shape::convex_polygon(tri.to_vec(), stroke.color, stroke));

    // 左端箭头头（仅双箭头）
    if double {
        let tri2 = arrow_head(end, start, head_len);
        if let Some(f) = fill {
            painter.add(Shape::convex_polygon(tri2.to_vec(), f, Stroke::NONE));
        }
        painter.add(Shape::convex_polygon(tri2.to_vec(), stroke.color, stroke));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 单元测试（纯几何，不依赖 Painter 上下文）
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 100.0))
    }

    #[test]
    fn parenthesis_has_expected_points() {
        assert_eq!(parenthesis_points(rect()).len(), 25);
    }

    #[test]
    fn parenthesis_stays_within_bounds() {
        let r = rect();
        for p in parenthesis_points(r) {
            assert!(p.x >= -0.5 && p.x <= 200.5, "x={} out of bounds", p.x);
            assert!(p.y >= -0.5 && p.y <= 100.5, "y={} out of bounds", p.y);
        }
    }

    #[test]
    fn bracket_has_three_segments() {
        assert_eq!(bracket_segments(rect()).len(), 3);
    }

    #[test]
    fn brace_has_four_beziers() {
        assert_eq!(brace_beziers(rect()).len(), 4);
    }

    #[test]
    fn arrow_head_tip_is_at_to() {
        let t = arrow_head(Pos2::new(0.0, 0.0), Pos2::new(100.0, 0.0), 10.0);
        assert_eq!(t[0], Pos2::new(100.0, 0.0));
        // 两个底点应关于连线（x 轴）对称。
        assert!((t[1].y + t[2].y).abs() < 1e-5);
        assert!(t[1].x < 100.0 && t[2].x < 100.0);
    }

    #[test]
    fn arrow_head_points_along_direction() {
        // 竖直向上箭头：尖端在 `to`，底点在 `to` 下方。
        let from = Pos2::new(50.0, 100.0);
        let to = Pos2::new(50.0, 0.0);
        let t = arrow_head(from, to, 12.0);
        assert_eq!(t[0], to);
        assert!(t[1].y > to.y && t[2].y > to.y);
    }

    #[test]
    fn polygon_vertex_count_matches_sides() {
        for sides in 3..=12_u8 {
            let pts = polygon_vertices(Pos2::ZERO, 40.0, sides, 90.0);
            assert_eq!(pts.len(), sides as usize, "sides={sides} 应产生等量顶点");
        }
    }

    #[test]
    fn polygon_radius_correct() {
        let r = 40.0;
        let pts = polygon_vertices(Pos2::ZERO, r, 6, 90.0);
        assert_eq!(pts.len(), 6);
        for p in &pts {
            let d = p.distance(Pos2::ZERO);
            assert!((d - r).abs() < 1e-3, "顶点 {p:?} 到中心距离应等于半径 {r}，实际 {d}");
        }
    }
}
