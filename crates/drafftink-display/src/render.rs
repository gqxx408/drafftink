//! Canvas rendering — grid, elements, selection handles, preview.
//!
//! All coordinate transforms go through `Camera`. Never do manual
//! world → screen math outside of this module.

use drafftink_core::model::{CoursewareDoc, Element, ShapeType};
use drafftink_core::Camera;
use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};
use kurbo::{BezPath, PathEl, Point as KPoint};
use lyon::math::Point as LyonPoint;
use lyon::path::Path as LyonPath;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, LineCap, LineJoin,
    StrokeOptions, StrokeTessellator, StrokeVertex, VertexBuffers,
};

use crate::interaction::InteractionState;

// ---------------------------------------------------------------------------
// Grid constants
// ---------------------------------------------------------------------------

#[allow(dead_code)]
const GRID_MAJOR: f32 = 100.0;
#[allow(dead_code)]
const GRID_MINOR: f32 = 50.0;
#[allow(dead_code)]
const GRID_MAJOR_COLOR: Color32 = Color32::from_rgb(0xD8, 0xD8, 0xD8);
#[allow(dead_code)]
const GRID_MINOR_COLOR: Color32 = Color32::from_rgb(0xEC, 0xEC, 0xEC);

// ---------------------------------------------------------------------------
// Selection handle constants
// ---------------------------------------------------------------------------

const HANDLE_SIZE: f32 = 7.0;
const HANDLE_FILL: Color32 = Color32::WHITE;
const HANDLE_STROKE: Color32 = Color32::from_rgb(0x3A, 0x86, 0xFF);
const HANDLE_STROKE_W: f32 = 2.0;

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

const CANVAS_BG: Color32 = Color32::from_rgb(0x1A, 0x3C, 0x1A);
const SELECTION_STROKE: Color32 = Color32::from_rgb(0x3A, 0x86, 0xFF);

/// Draw the full canvas: background, page, elements, selection, preview.
pub fn render_canvas(
    painter: &Painter,
    doc: &CoursewareDoc,
    camera: &Camera,
    interaction: &InteractionState,
    current_page: usize,
) {
    let clip = painter.clip_rect();

    // 1. Background
    painter.rect_filled(clip, 0.0, CANVAS_BG);

    // 2. Elements (sorted by z-order) from current page
    let elements_vec: &Vec<Element> = if !doc.pages.is_empty() {
        doc.pages
            .get(current_page)
            .map(|p| &p.elements)
            .unwrap_or(&doc.elements)
    } else if !doc.elements.is_empty() {
        &doc.elements
    } else {
        &doc.elements
    };
    if !elements_vec.is_empty() {
        log::debug!("Canvas rendering {} elements", elements_vec.len());
    }
    let mut elements: Vec<(i32, &Element)> =
        elements_vec.iter().map(|e| (e.base().z_order, e)).collect();
    elements.sort_by_key(|(z, _)| *z);
    for (_, elem) in &elements {
        if elem.base().visible {
            draw_element(painter, camera, elem);
        }
    }

    // 3. Drawing preview (when dragging to create a new shape)
    if interaction.is_drawing
        && matches!(interaction.mode, crate::interaction::ToolMode::DrawShape(_))
    {
        if let Some(rect) = interaction.draw_rect() {
            let st = interaction.mode;
            draw_drag_preview(painter, camera, &rect, st);
        }
    }

    // 4. Selection handles
    for id in &interaction.selected_ids {
        if let Some(elem) = doc.get(*id) {
            if interaction.editing_text_id.as_ref() == Some(id) {
                continue;
            }
            draw_selection(painter, camera, elem);
        }
    }
}

// ---------------------------------------------------------------------------
// Element drawing
// ---------------------------------------------------------------------------

fn draw_element(painter: &Painter, camera: &Camera, elem: &Element) {
    let base = elem.base();
    let opacity = base.opacity;

    match elem {
        Element::Shape(shape) => draw_shape(painter, camera, shape, opacity),
        Element::Text(text) => draw_text(painter, camera, text, opacity),
        Element::Image(_img) => {
            let tl = camera.world_to_screen(base.position);
            let br = camera.world_to_screen([
                base.position[0] + base.size[0],
                base.position[1] + base.size[1],
            ]);
            let rect = Rect::from_min_max(tl, br);
            painter.rect_filled(rect, 4.0, Color32::from_rgb(0xE0, 0xE0, 0xE0));
            painter.rect_stroke(rect, 4.0, Stroke::new(1.0, Color32::from_gray(180)));
            painter.line_segment([tl, br], Stroke::new(1.0, Color32::from_gray(180)));
            painter.line_segment(
                [Pos2::new(br.x, tl.y), Pos2::new(tl.x, br.y)],
                Stroke::new(1.0, Color32::from_gray(180)),
            );
        }
        Element::Path(path) => draw_path(painter, camera, path, opacity),
        Element::SvgShape(svg) => draw_svg_shape(painter, camera, svg, opacity),
    }
}

fn draw_shape(
    painter: &Painter,
    camera: &Camera,
    shape: &drafftink_core::model::ShapeElement,
    opacity: f32,
) {
    let base = &shape.base;
    let tl = camera.world_to_screen(base.position);
    let br = camera.world_to_screen([
        base.position[0] + base.size[0],
        base.position[1] + base.size[1],
    ]);
    let rect = Rect::from_min_max(tl, br);

    let fill = multiply_alpha(base.fill_color, opacity);
    let stroke_color = multiply_alpha(base.stroke_color, opacity);
    let stroke_width = base.stroke_width * camera.zoom;
    let stroke = Stroke::new(stroke_width, stroke_color);

    match shape.shape_type {
        ShapeType::Rectangle => {
            painter.rect_filled(rect, 0.0, fill);
            if base.stroke_width > 0.0 {
                painter.rect_stroke(rect, 0.0, stroke);
            }
        }
        ShapeType::Ellipse => {
            let r = rect.size().x.min(rect.size().y) * 0.5;
            painter.rect_filled(rect, r, fill);
            if base.stroke_width > 0.0 {
                painter.rect_stroke(rect, r, stroke);
            }
        }
        ShapeType::Line => {
            let end = br;
            painter.line_segment([tl, end], stroke);
        }
        ShapeType::Arrow => {
            let start = tl;
            let end = br;
            painter.line_segment([start, end], stroke);
            if shape.has_end_arrow {
                draw_arrow_head(painter, end, start, stroke_color, stroke_width);
            }
            if shape.has_start_arrow {
                draw_arrow_head(painter, start, end, stroke_color, stroke_width);
            }
        }
        ShapeType::Bracket => {
            draw_bracket(painter, rect, stroke);
        }
        ShapeType::Brace => {
            draw_brace(painter, rect, stroke, shape.scale_y);
        }
        ShapeType::Fan => {
            // Fan shapes are rendered via SvgShape path (not through draw_shape).
            // This arm exists only for exhaustiveness and should never be hit.
            log::warn!("ShapeType::Fan reached draw_shape (should be SvgShape)");
        }
    }
}

/// Draw a filled arrow head (triangle) pointing at `tip`, coming from `from`.
fn draw_arrow_head(painter: &Painter, tip: Pos2, from: Pos2, color: Color32, stroke_width: f32) {
    let dir = (tip - from).normalized();
    let perp = Vec2::new(-dir.y, dir.x);
    // Arrow size scales with stroke width, with sensible minimum/maximum
    let head_len = (10.0 + stroke_width * 4.0).clamp(8.0, 24.0);
    let head_w = head_len * 0.5;
    let p_back = tip - dir * head_len;
    let p1 = tip;
    let p2 = p_back + perp * head_w;
    let p3 = p_back - perp * head_w;
    // Filled triangle
    painter.add(Shape::convex_polygon(
        vec![p1, p2, p3],
        color,
        Stroke::new(1.0, color),
    ));
}

/// Draw a square bracket [ opening to the right (left vertical + top/bottom ticks).
///
/// Interprets `rect` as the bounding box: left edge is the vertical bar,
/// right edge is where the top/bottom ticks end.
fn draw_bracket(painter: &Painter, rect: Rect, stroke: Stroke) {
    let top = rect.top();
    let bottom = rect.bottom();
    let left = rect.left();
    let right = rect.right();
    // Vertical bar along the left
    painter.line_segment([Pos2::new(left, top), Pos2::new(left, bottom)], stroke);
    // Top tick (→)
    let tick_len = (right - left).max(6.0);
    painter.line_segment(
        [Pos2::new(left, top), Pos2::new(left + tick_len, top)],
        stroke,
    );
    // Bottom tick (→)
    painter.line_segment(
        [Pos2::new(left, bottom), Pos2::new(left + tick_len, bottom)],
        stroke,
    );
}

/// Draw a curly brace { opening to the RIGHT (cusp/point on the LEFT, tips on
/// the RIGHT), using two cubic Bézier curves meeting at the cusp. This matches
/// Seewo's preset brace geometry. `scale_y` (0..1) controls how far the control
/// points bulge toward the mouth (tip) side — Seewo's default is 0.2.
fn draw_brace(painter: &Painter, rect: Rect, stroke: Stroke, scale_y: f32) {
    let back = rect.left(); // cusp side (left)
    let mouth = rect.right().max(back + 4.0); // tips side (right)
    let top = rect.top();
    let bottom = rect.bottom();
    let mid_y = (top + bottom) * 0.5;
    let off = (mouth - back) * scale_y.max(0.1).min(1.0);

    let bez = |p0: Pos2, p1: Pos2, p2: Pos2, p3: Pos2| {
        painter.add(Shape::CubicBezier(egui::epaint::CubicBezierShape {
            points: [p0, p1, p2, p3],
            closed: false,
            fill: Color32::TRANSPARENT,
            stroke: stroke.into(),
        }));
    };
    // Upper arc: top-right tip → cusp (left-middle), bulging right.
    bez(
        Pos2::new(mouth, top),
        Pos2::new(mouth, top + off),
        Pos2::new(back + off, mid_y - off),
        Pos2::new(back, mid_y),
    );
    // Lower arc: cusp (left-middle) → bottom-right tip, bulging right.
    bez(
        Pos2::new(back, mid_y),
        Pos2::new(back + off, mid_y + off),
        Pos2::new(mouth, bottom - off),
        Pos2::new(mouth, bottom),
    );
}

fn draw_svg_shape(
    painter: &Painter,
    camera: &Camera,
    svg: &drafftink_core::model::SvgShapeElement,
    opacity: f32,
) {
    let base = &svg.base;
    let stroke_color = multiply_alpha(base.stroke_color, opacity);
    let fill_color = multiply_alpha(base.fill_color, opacity);
    let stroke_width = (base.stroke_width * camera.zoom).max(0.5);

    // Parse SVG path (strip optional fill-rule prefix like "F1" or "F0")
    let path_str: String = svg
        .svg_path
        .trim()
        .trim_start_matches("F1")
        .trim_start_matches("F0")
        .trim()
        .to_string();
    if path_str.is_empty() {
        return;
    }

    let bez = match BezPath::from_svg(&path_str) {
        Ok(b) => b,
        Err(e) => {
            log::warn!(
                "SVG path parse error: {:?} (path starts with: {:?})",
                e,
                &path_str[..path_str.len().min(60)]
            );
            return;
        }
    };

    // Auto-detect closed path: SVG 'z'/'Z' close command at end, or explicit flag.
    let path_ends_with_z = path_str.trim_end().ends_with('z') || path_str.trim_end().ends_with('Z');
    let is_closed = svg.is_closed || path_ends_with_z;

    // Path coordinates are in the shape's local coordinate space (relative to base.position).
    // Translate to world coords by adding (ox, oy) before camera transform.
    let ox = base.position[0];
    let oy = base.position[1];
    let local_to_screen = |p: KPoint| -> Pos2 {
        let wx = p.x as f32 + ox;
        let wy = p.y as f32 + oy;
        camera.world_to_screen([wx, wy])
    };

    // Use kurbo's free flatten() function to convert all curves (Q/C) to line segments.
    // Tolerance 0.05 local units gives sub-pixel smoothness for curved arrows.
    let tolerance = 0.05_f64;
    let mut lyon_builder = LyonPath::builder();
    let mut first_point: Option<Pos2> = None;
    let mut last_point: Option<Pos2> = None;
    let mut first_tangent: Option<Pos2> = None;
    let mut last_tangent: Option<Pos2> = None;
    let mut prev_screen: Option<Pos2> = None;
    let mut subpath_started = false;

    kurbo::flatten(bez.iter(), tolerance, |el| {
        match el {
            PathEl::MoveTo(p) => {
                if subpath_started {
                    lyon_builder.end(false);
                }
                let sp = local_to_screen(p);
                lyon_builder.begin(LyonPoint::new(sp.x, sp.y));
                subpath_started = true;
                if first_point.is_none() {
                    first_point = Some(sp);
                }
                last_point = Some(sp);
                prev_screen = Some(sp);
            }
            PathEl::LineTo(p) => {
                let sp = local_to_screen(p);
                lyon_builder.line_to(LyonPoint::new(sp.x, sp.y));
                if let Some(prev) = prev_screen {
                    if first_tangent.is_none() && first_point.is_some() {
                        first_tangent = Some(sp);
                    }
                    last_tangent = Some(prev);
                }
                last_point = Some(sp);
                prev_screen = Some(sp);
            }
            PathEl::ClosePath => {
                if subpath_started {
                    lyon_builder.close();
                    subpath_started = false;
                }
            }
            // flatten() only yields MoveTo/LineTo/ClosePath — no QuadTo/CurveTo remain
            PathEl::QuadTo(..) | PathEl::CurveTo(..) => {}
        }
    });
    if subpath_started {
        lyon_builder.end(is_closed);
    }

    let lyon_path = lyon_builder.build();

    // ── Fill (Fan / Ellipse / Circle / closed shapes) using lyon FillTessellator ──
    if is_closed && fill_color != Color32::TRANSPARENT {
        let mut buffers: VertexBuffers<LyonPoint, u32> = VertexBuffers::new();
        let fill_options = FillOptions::default()
            .with_tolerance(0.01)
            .with_fill_rule(FillRule::NonZero);
        let mut tess = FillTessellator::new();
        let mut buffers_builder = BuffersBuilder::new(&mut buffers, |v: FillVertex| v.position());
        if tess
            .tessellate_path(&lyon_path, &fill_options, &mut buffers_builder)
            .is_ok()
        {
            build_and_add_mesh(painter, &buffers, fill_color);
        } else {
            log::warn!(
                "lyon fill tessellation failed for SvgShape (path_len={})",
                path_str.len()
            );
        }
    }

    // ── Stroke using lyon StrokeTessellator ──
    if stroke_width > 0.0 && stroke_color != Color32::TRANSPARENT {
        let mut buffers: VertexBuffers<LyonPoint, u32> = VertexBuffers::new();
        let stroke_opts = StrokeOptions::default()
            .with_line_width(stroke_width)
            .with_tolerance(0.01)
            .with_line_cap(LineCap::Round)
            .with_line_join(LineJoin::Round);
        let mut tess = StrokeTessellator::new();
        let mut buffers_builder = BuffersBuilder::new(&mut buffers, |v: StrokeVertex| v.position());
        if tess
            .tessellate_path(&lyon_path, &stroke_opts, &mut buffers_builder)
            .is_ok()
        {
            build_and_add_mesh(painter, &buffers, stroke_color);
        } else {
            log::warn!("lyon stroke tessellation failed for SvgShape");
        }
    }

    // ── Arrow heads (based on first/last points of the path) ──
    if let (Some(first), Some(last)) = (first_point, last_point) {
        if svg.has_end_arrow && first != last {
            let tip = last;
            let from = last_tangent.unwrap_or(first);
            draw_arrow_head(painter, tip, from, stroke_color, stroke_width);
        }
        if svg.has_start_arrow && first != last {
            let tip = first;
            let to = first_tangent.unwrap_or(last);
            draw_arrow_head(painter, tip, to, stroke_color, stroke_width);
        }
    }
}

/// Helper: build an egui Mesh from lyon vertex/index buffers and add it to the painter.
fn build_and_add_mesh(painter: &Painter, buffers: &VertexBuffers<LyonPoint, u32>, color: Color32) {
    let mut mesh = Mesh::default();
    mesh.reserve_vertices(buffers.vertices.len());
    mesh.indices.reserve(buffers.indices.len());

    for v in &buffers.vertices {
        mesh.vertices.push(Vertex {
            pos: Pos2::new(v.x, v.y),
            uv: WHITE_UV,
            color,
        });
    }
    for idx in &buffers.indices {
        mesh.indices.push(*idx);
    }

    painter.add(Shape::mesh(mesh));
}

fn draw_text(
    painter: &Painter,
    camera: &Camera,
    text: &drafftink_core::model::TextElement,
    opacity: f32,
) {
    let base = &text.base;
    let pos = camera.world_to_screen(base.position);
    let color = multiply_alpha(base.fill_color, opacity);

    let font_id = egui::FontId::new(text.font_size * camera.zoom, egui::FontFamily::Proportional);
    painter.text(pos, egui::Align2::LEFT_TOP, &text.text, font_id, color);
}

fn draw_path(
    painter: &Painter,
    camera: &Camera,
    path: &drafftink_core::model::PathElement,
    opacity: f32,
) {
    let base = &path.base;
    if path.points.len() < 2 {
        return;
    }
    let color = multiply_alpha(base.stroke_color, opacity);
    let stroke = Stroke::new(base.stroke_width * camera.zoom, color);

    let screen_pts: Vec<Pos2> = path
        .points
        .iter()
        .map(|p| camera.world_to_screen(*p))
        .collect();

    for window in screen_pts.windows(2) {
        painter.line_segment([window[0], window[1]], stroke);
    }

    if path.is_closed && screen_pts.len() >= 3 {
        painter.line_segment([screen_pts[screen_pts.len() - 1], screen_pts[0]], stroke);
    }
}

// ---------------------------------------------------------------------------
// Drawing preview (while dragging to create)
// ---------------------------------------------------------------------------

fn draw_drag_preview(
    painter: &Painter,
    camera: &Camera,
    world_rect: &[f32; 4],
    mode: crate::interaction::ToolMode,
) {
    let tl = camera.world_to_screen([world_rect[0], world_rect[1]]);
    let br = camera.world_to_screen([world_rect[2], world_rect[3]]);
    let rect = Rect::from_min_max(tl, br);

    let preview_fill = Color32::from_rgba_unmultiplied(58, 134, 255, 40);
    let preview_stroke = Stroke::new(2.0, Color32::from_rgb(0x3A, 0x86, 0xFF));

    match mode {
        crate::interaction::ToolMode::DrawShape(ShapeType::Rectangle) => {
            painter.rect_filled(rect, 0.0, preview_fill);
            painter.rect_stroke(rect, 0.0, preview_stroke);
        }
        crate::interaction::ToolMode::DrawShape(ShapeType::Ellipse) => {
            let r = rect.size().x.min(rect.size().y) * 0.5;
            painter.rect_filled(rect, r, preview_fill);
            painter.rect_stroke(rect, r, preview_stroke);
        }
        crate::interaction::ToolMode::DrawShape(ShapeType::Line)
        | crate::interaction::ToolMode::DrawShape(ShapeType::Arrow) => {
            painter.line_segment([tl, br], preview_stroke);
            if matches!(
                mode,
                crate::interaction::ToolMode::DrawShape(ShapeType::Arrow)
            ) {
                draw_arrow_head(painter, br, tl, preview_stroke.color, preview_stroke.width);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------

fn draw_selection(painter: &Painter, camera: &Camera, elem: &Element) {
    let base = elem.base();
    let tl = camera.world_to_screen(base.position);
    let br = camera.world_to_screen([
        base.position[0] + base.size[0],
        base.position[1] + base.size[1],
    ]);

    let rect = Rect::from_min_max(tl, br);
    painter.rect_stroke(rect.expand(2.0), 0.0, Stroke::new(1.5, SELECTION_STROKE));

    let handles = [
        tl,
        Pos2::new((tl.x + br.x) * 0.5, tl.y),
        Pos2::new(br.x, tl.y),
        Pos2::new(br.x, (tl.y + br.y) * 0.5),
        br,
        Pos2::new((tl.x + br.x) * 0.5, br.y),
        Pos2::new(tl.x, br.y),
        Pos2::new(tl.x, (tl.y + br.y) * 0.5),
    ];

    for handle in &handles {
        let hr = Rect::from_center_size(*handle, Vec2::new(HANDLE_SIZE, HANDLE_SIZE));
        painter.rect_filled(hr, 1.0, HANDLE_FILL);
        painter.rect_stroke(hr, 1.0, Stroke::new(HANDLE_STROKE_W, HANDLE_STROKE));
    }

    let rot_center = Pos2::new((tl.x + br.x) * 0.5, tl.y - 20.0);
    painter.line_segment(
        [Pos2::new((tl.x + br.x) * 0.5, tl.y), rot_center],
        Stroke::new(1.5, SELECTION_STROKE),
    );
    painter.circle_filled(rot_center, 4.0, HANDLE_FILL);
    painter.circle_stroke(rot_center, 4.0, Stroke::new(HANDLE_STROKE_W, HANDLE_STROKE));
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn multiply_alpha(color: Color32, opacity: f32) -> Color32 {
    let a = (color.a() as f32 * opacity).round() as u8;
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a)
}
