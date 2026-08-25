//! Canvas rendering — grid, elements, selection handles, preview.
//!
//! All coordinate transforms go through `Camera`.  Never do manual
#![allow(dead_code)]
//! world → screen math outside of this module.

use drafftink_core::model::{CoursewareDoc, Element, ShapeType};
use drafftink_core::Camera;
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};

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

const CANVAS_BG: Color32 = Color32::from_rgb(0xE8, 0xE8, 0xE8);
const SELECTION_STROKE: Color32 = Color32::from_rgb(0x3A, 0x86, 0xFF);

/// Draw the full canvas: background, grid, page, elements, selection, preview.
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

    // 2. Grid removed — full green background
    // draw_grid(painter, camera, clip);

    // 3. Page boundary removed
    // draw_page_boundary(painter, camera, doc);

    // 4. Elements (sorted) from the CURRENT page.
    // Multi-page docs render `pages[current_page]`; legacy single-page docs fall
    // back to `doc.elements`. We MUST respect `current_page` — otherwise every
    // page shows page 0's content after a NewPage / page switch.
    let elements_vec: &Vec<Element> = if !doc.pages.is_empty() {
        doc.pages
            .get(current_page)
            .map(|p| &p.elements)
            .unwrap_or(&doc.elements)
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

    // 5. Drawing preview (when drawing a new shape)
    if interaction.is_drawing
        && matches!(interaction.mode, crate::interaction::ToolMode::DrawShape(_))
    {
        if let Some(rect) = interaction.draw_rect() {
            let st = interaction.mode;
            draw_drag_preview(painter, camera, &rect, st);
        }
    }

    // 6. Selection handles
    for id in &interaction.selected_ids {
        if let Some(elem) = doc.get(*id) {
            if interaction.editing_text_id.as_ref() == Some(id) {
                // Don't draw handles while editing text
                continue;
            }
            draw_selection(painter, camera, elem);
        }
    }
}

// ---------------------------------------------------------------------------
// Background & grid
// ---------------------------------------------------------------------------

#[allow(dead_code)]
fn draw_grid(painter: &Painter, camera: &Camera, clip: Rect) {
    let [vl, vt, vr, vb] = camera.visible_world_bounds();
    let zoom = camera.zoom;

    // Choose spacing based on zoom level
    let (major_spacing, minor_spacing) = if zoom < 0.2 {
        (GRID_MAJOR * 4.0, GRID_MINOR * 4.0)
    } else if zoom < 0.5 {
        (GRID_MAJOR * 2.0, GRID_MINOR * 2.0)
    } else {
        (GRID_MAJOR, GRID_MINOR)
    };

    // Helper to draw vertical/horizontal lines
    let draw_line = |painter: &Painter, p1: Pos2, p2: Pos2, color: Color32| {
        if clip.contains(p1) || clip.contains(p2) || clip.intersects(Rect::from_two_pos(p1, p2)) {
            painter.line_segment([p1, p2], Stroke::new(1.0, color));
        }
    };

    // Minor grid lines
    let color = GRID_MINOR_COLOR;
    let mut x = (vl / minor_spacing).floor() * minor_spacing;
    while x <= vr + minor_spacing {
        let p1 = camera.world_to_screen([x, vt]);
        let p2 = camera.world_to_screen([x, vb]);
        draw_line(painter, p1, p2, color);
        x += minor_spacing;
    }
    let mut y = (vt / minor_spacing).floor() * minor_spacing;
    while y <= vb + minor_spacing {
        let p1 = camera.world_to_screen([vl, y]);
        let p2 = camera.world_to_screen([vr, y]);
        draw_line(painter, p1, p2, color);
        y += minor_spacing;
    }

    // Major grid lines (on top)
    let color = GRID_MAJOR_COLOR;
    let mut x = (vl / major_spacing).floor() * major_spacing;
    while x <= vr + major_spacing {
        let p1 = camera.world_to_screen([x, vt]);
        let p2 = camera.world_to_screen([x, vb]);
        draw_line(painter, p1, p2, color);
        x += major_spacing;
    }
    let mut y = (vt / major_spacing).floor() * major_spacing;
    while y <= vb + major_spacing {
        let p1 = camera.world_to_screen([vl, y]);
        let p2 = camera.world_to_screen([vr, y]);
        draw_line(painter, p1, p2, color);
        y += major_spacing;
    }
}

#[allow(dead_code)]
fn draw_page_boundary(painter: &Painter, camera: &Camera, doc: &CoursewareDoc) {
    let tl = camera.world_to_screen([0.0, 0.0]);
    let br = camera.world_to_screen(doc.page_size);

    let page_rect = Rect::from_min_max(tl, br);

    // Page border only — background blends with canvas
    painter.rect_stroke(page_rect, 0.0, Stroke::new(1.0, Color32::from_gray(200)));
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
            // Image rendering requires texture handle — handled in app.rs
            // Draw a placeholder rect for now
            let tl = camera.world_to_screen(base.position);
            let br = camera.world_to_screen([
                base.position[0] + base.size[0],
                base.position[1] + base.size[1],
            ]);
            let rect = Rect::from_min_max(tl, br);
            painter.rect_filled(rect, 4.0, Color32::from_rgb(0xE0, 0xE0, 0xE0));
            painter.rect_stroke(rect, 4.0, Stroke::new(1.0, Color32::from_gray(180)));
            // Cross pattern to indicate image placeholder
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
    let stroke = Stroke::new(base.stroke_width * camera.zoom, stroke_color);

    match shape.shape_type {
        ShapeType::Rectangle => {
            painter.rect_filled(rect, 0.0, fill);
            if base.stroke_width > 0.0 {
                painter.rect_stroke(rect, 0.0, stroke);
            }
        }
        ShapeType::Ellipse => {
            // Approximate ellipse with rounded rect
            let r = rect.size().x.min(rect.size().y) * 0.5;
            painter.rect_filled(rect, r, fill);
            if base.stroke_width > 0.0 {
                painter.rect_stroke(rect, r, stroke);
            }
        }
        ShapeType::Line => {
            let end = camera.world_to_screen([
                base.position[0] + base.size[0],
                base.position[1] + base.size[1],
            ]);
            painter.line_segment([tl, end], stroke);
        }
        ShapeType::Arrow => {
            let end = camera.world_to_screen([
                base.position[0] + base.size[0],
                base.position[1] + base.size[1],
            ]);
            painter.line_segment([tl, end], stroke);
            // Arrow head
            draw_arrow_head(
                painter,
                tl,
                end,
                stroke_color,
                base.stroke_width * camera.zoom,
            );
        }
        ShapeType::Bracket => {
            draw_bracket(painter, rect, stroke);
        }
        ShapeType::Brace => {
            draw_brace(painter, rect, stroke, shape.scale_y);
        }
        ShapeType::Fan => {
            // placeholder — Fan shapes go through SvgShape
        }
    }
}

fn draw_arrow_head(painter: &Painter, from: Pos2, to: Pos2, color: Color32, width: f32) {
    let dir = (to - from).normalized();
    let perp = Vec2::new(-dir.y, dir.x);
    let head_size = 12.0 * (width / 2.0).max(1.0);
    let p1 = to;
    let p2 = to - dir * head_size + perp * head_size * 0.5;
    let p3 = to - dir * head_size - perp * head_size * 0.5;
    painter.add(Shape::convex_polygon(
        vec![p1, p2, p3],
        color,
        Stroke::new(1.0, color),
    ));
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

    // Diagnostic — one-time log per element
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        static LOGGED: AtomicBool = AtomicBool::new(false);
        if !LOGGED.swap(true, Ordering::Relaxed) {
            log::info!(
                "draw_text: {:?} @ {:?} fill_raw={:?} after_mul={:?} font={:.0}px zoom={:.2} op={:.2}",
                &text.text[..text.text.len().min(60)],
                pos, base.fill_color, color, text.font_size, camera.zoom, opacity
            );
        }
    }

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

    // Selection rect
    let rect = Rect::from_min_max(tl, br);
    painter.rect_stroke(rect.expand(2.0), 0.0, Stroke::new(1.5, SELECTION_STROKE));

    // 8 resize handles (corners + edges)
    let handles = [
        tl,                                   // top-left
        Pos2::new((tl.x + br.x) * 0.5, tl.y), // top-center
        Pos2::new(br.x, tl.y),                // top-right
        Pos2::new(br.x, (tl.y + br.y) * 0.5), // right-center
        br,                                   // bottom-right
        Pos2::new((tl.x + br.x) * 0.5, br.y), // bottom-center
        Pos2::new(tl.x, br.y),                // bottom-left
        Pos2::new(tl.x, (tl.y + br.y) * 0.5), // left-center
    ];

    for handle in &handles {
        let hr = Rect::from_center_size(*handle, Vec2::new(HANDLE_SIZE, HANDLE_SIZE));
        painter.rect_filled(hr, 1.0, HANDLE_FILL);
        painter.rect_stroke(hr, 1.0, Stroke::new(HANDLE_STROKE_W, HANDLE_STROKE));
    }

    // Rotation handle (top line + circle above)
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

// ---------------------------------------------------------------------------
// Bracket / Brace drawing
// ---------------------------------------------------------------------------

/// Draw a square bracket `[` opening to the right (left vertical + top/bottom ticks).
fn draw_bracket(painter: &Painter, rect: Rect, stroke: Stroke) {
    let top = rect.top();
    let bottom = rect.bottom();
    let left = rect.left();
    let right = rect.right();
    // Vertical bar along the left
    painter.line_segment([Pos2::new(left, top), Pos2::new(left, bottom)], stroke);
    // Top tick
    let tick_len = (right - left).max(6.0);
    painter.line_segment(
        [Pos2::new(left, top), Pos2::new(left + tick_len, top)],
        stroke,
    );
    // Bottom tick
    painter.line_segment(
        [Pos2::new(left, bottom), Pos2::new(left + tick_len, bottom)],
        stroke,
    );
}

/// Draw a curly brace `{` opening to the right using cubic Bézier curves.
/// `scale_y` (0.0–1.0) controls the curvature of the middle cusp.
fn draw_brace(painter: &Painter, rect: Rect, stroke: Stroke, scale_y: f32) {
    let x = rect.left();
    let x_mid = rect.right().max(x + 4.0);
    let top = rect.top();
    let bottom = rect.bottom();
    let mid_y = (top + bottom) * 0.5;
    let h = (bottom - top).max(2.0);
    let curv = (x_mid - x) * scale_y.max(0.1).min(1.0);
    let q1 = top + h * 0.25;
    let q3 = top + h * 0.75;
    let cusp = Pos2::new(x_mid, mid_y);

    let bez = |p0: Pos2, p1: Pos2, p2: Pos2, p3: Pos2| {
        painter.add(Shape::CubicBezier(egui::epaint::CubicBezierShape {
            points: [p0, p1, p2, p3],
            closed: false,
            fill: Color32::TRANSPARENT,
            stroke: stroke.into(),
        }));
    };
    // Top → q1
    bez(
        Pos2::new(x, top),
        Pos2::new(x + curv, top),
        Pos2::new(x + curv, q1 - h * 0.05),
        Pos2::new(x_mid - curv * 0.3, q1),
    );
    // q1 → cusp
    bez(
        Pos2::new(x_mid - curv * 0.3, q1),
        Pos2::new(x_mid, q1 + h * 0.05),
        Pos2::new(x_mid, mid_y - h * 0.08),
        cusp,
    );
    // cusp → q3
    bez(
        cusp,
        Pos2::new(x_mid, mid_y + h * 0.08),
        Pos2::new(x_mid, q3 - h * 0.05),
        Pos2::new(x_mid - curv * 0.3, q3),
    );
    // q3 → bottom
    bez(
        Pos2::new(x_mid - curv * 0.3, q3),
        Pos2::new(x + curv, q3 + h * 0.05),
        Pos2::new(x + curv, bottom),
        Pos2::new(x, bottom),
    );
}

// ---------------------------------------------------------------------------
// SVG shape placeholder
// ---------------------------------------------------------------------------

/// Placeholder renderer for `SvgShapeElement` (kurbo not available in this crate).
/// Draws a stroked rectangle with a diagonal to indicate an SVG shape.
fn draw_svg_shape(
    painter: &Painter,
    camera: &Camera,
    svg: &drafftink_core::model::SvgShapeElement,
    opacity: f32,
) {
    let base = &svg.base;
    let tl = camera.world_to_screen(base.position);
    let br = camera.world_to_screen([
        base.position[0] + base.size[0],
        base.position[1] + base.size[1],
    ]);
    let rect = Rect::from_min_max(tl, br);
    let stroke_color = multiply_alpha(base.stroke_color, opacity);
    let stroke = Stroke::new((base.stroke_width * camera.zoom).max(0.5), stroke_color);
    painter.rect_stroke(rect, 0.0, stroke);
    painter.line_segment([tl, br], stroke);
}
