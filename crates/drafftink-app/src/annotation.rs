//! Annotation / teaching tools module.
//!
//! Provides Pen, Highlighter, Eraser, LaserPointer, ClearScreen tools
#![allow(dead_code)]
//! that work on top of the canvas.  Supports mouse right-click erase
//! and touch two-finger erase.
//!
//! All stroke data is kept in screen-space pixel coordinates (so that
//! annotation is independent of camera zoom/pan).
//!
//! A stroke is composed of multiple **segments** — each segment is a
//! contiguous sequence of points.  When the eraser cuts through the middle
//! of a segment, it is split into two (or more) sub-segments; the erased
//! portion is discarded.

use egui::{Color32, Pos2};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

// ---------------------------------------------------------------------------
// Thread-local temporary buffers (reuse to avoid allocations)
// ---------------------------------------------------------------------------

thread_local! {
    /// Scratch buffer for densified points.  Pre-allocated to 2K points.
    static TMP_DENSIFY: RefCell<Vec<Pos2>> = RefCell::new(Vec::with_capacity(2048));
    /// Scratch buffer for inside/outside classification.
    static TMP_INSIDE: RefCell<Vec<bool>> = RefCell::new(Vec::with_capacity(2048));
    /// Scratch buffer for the current working sub-segment.
    static TMP_CUR: RefCell<Vec<Pos2>> = RefCell::new(Vec::with_capacity(512));
}

// ---------------------------------------------------------------------------
// Serde helpers for egui types
// ---------------------------------------------------------------------------

mod egui_serde {
    use egui::{Color32, Pos2};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[allow(dead_code)]
    pub fn serialize_pos2<S: Serializer>(pos: &Pos2, s: S) -> Result<S::Ok, S::Error> {
        [pos.x, pos.y].serialize(s)
    }

    #[allow(dead_code)]
    pub fn deserialize_pos2<'de, D: Deserializer<'de>>(d: D) -> Result<Pos2, D::Error> {
        let [x, y] = <[f32; 2]>::deserialize(d)?;
        Ok(Pos2::new(x, y))
    }

    pub fn serialize_color32<S: Serializer>(c: &Color32, s: S) -> Result<S::Ok, S::Error> {
        [c.r(), c.g(), c.b(), c.a()].serialize(s)
    }

    pub fn deserialize_color32<'de, D: Deserializer<'de>>(d: D) -> Result<Color32, D::Error> {
        let rgba = <[u8; 4]>::deserialize(d)?;
        Ok(Color32::from_rgba_unmultiplied(
            rgba[0], rgba[1], rgba[2], rgba[3],
        ))
    }
}

// ---------------------------------------------------------------------------
// Serde helpers for Vec<Vec<Pos2>> (with u16 coordinate compression)
// ---------------------------------------------------------------------------

/// Upper bound for screen coordinates (covers 4K displays).
/// Maps [0, CANVAS_MAX] → [0, u16::MAX], giving ~0.06 px precision at 1920 px.
const CANVAS_MAX: f32 = 3840.0;

fn serialize_segments<S: serde::Serializer>(
    segments: &[Vec<Pos2>],
    s: S,
) -> Result<S::Ok, S::Error> {
    use serde::Serialize;
    // Each segment becomes a flat Vec<u16>: [x0,y0, x1,y1, ...]
    let raw: Vec<Vec<u16>> = segments
        .iter()
        .map(|seg| {
            let mut flat = Vec::with_capacity(seg.len() * 2);
            for p in seg {
                let x = ((p.x / CANVAS_MAX * 65535.0).round() as u32).min(65535) as u16;
                let y = ((p.y / CANVAS_MAX * 65535.0).round() as u32).min(65535) as u16;
                flat.push(x);
                flat.push(y);
            }
            flat.shrink_to_fit();
            flat
        })
        .collect();
    raw.serialize(s)
}

fn deserialize_segments<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<Vec<Pos2>>, D::Error> {
    use serde::Deserialize;
    let raw: Vec<Vec<u16>> = Vec::deserialize(d)?;
    Ok(raw
        .into_iter()
        .map(|flat| {
            let mut seg = Vec::with_capacity(flat.len() / 2);
            for chunk in flat.chunks_exact(2) {
                seg.push(Pos2::new(
                    chunk[0] as f32 / 65535.0 * CANVAS_MAX,
                    chunk[1] as f32 / 65535.0 * CANVAS_MAX,
                ));
            }
            seg
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Tool enumeration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum AnnotationTool {
    #[default]
    Pen,
    Highlighter,
    Eraser,
    LaserPointer,
    ClearScreen,
}

// ---------------------------------------------------------------------------
// Stroke data
// ---------------------------------------------------------------------------

/// A single annotation stroke, composed of one or more **segments**
/// (each segment is a contiguous polyline).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrokeData {
    #[serde(
        serialize_with = "serialize_segments",
        deserialize_with = "deserialize_segments"
    )]
    pub segments: Vec<Vec<Pos2>>,

    #[serde(
        serialize_with = "egui_serde::serialize_color32",
        deserialize_with = "egui_serde::deserialize_color32"
    )]
    pub color: Color32,

    pub thickness: f32,

    /// Tool that created this stroke (Pen or Highlighter).
    pub tool_type: AnnotationTool,
}

impl StrokeData {
    fn new(color: Color32, thickness: f32, tool_type: AnnotationTool) -> Self {
        Self {
            segments: vec![Vec::new()], // one empty segment to start drawing into
            color,
            thickness,
            tool_type,
        }
    }

    /// Push a point into the current (last) segment.
    fn push_point(&mut self, p: Pos2) {
        let last = self
            .segments
            .last_mut()
            .expect("StrokeData always has ≥1 segment");
        last.push(p);
    }

    /// Return a reference to the last point, if any.
    fn last_point(&self) -> Option<&Pos2> {
        self.segments.last().and_then(|s| s.last())
    }

    /// Total point count across all segments.
    fn total_points(&self) -> usize {
        self.segments.iter().map(|s| s.len()).sum()
    }

    /// Number of segments that have ≥ 2 points.
    fn valid_segment_count(&self) -> usize {
        self.segments.iter().filter(|s| s.len() >= 2).count()
    }

    // ------------------------------------------------------------------
    // Geometric cutting (eraser)
    // ------------------------------------------------------------------

    /// Erase the portion of this stroke that falls within the eraser circle.
    ///
    /// Each segment is first densified to ~1.5 px spacing, then each point is
    /// classified as inside/outside the eraser.  When the classification
    /// transitions between states, the **exact** circle-line intersection
    /// point is computed and inserted as a boundary vertex.  This produces
    /// pixel-smooth cuts aligned exactly with the eraser circle edge.
    ///
    /// After cutting, surviving segments are simplified with RDP (ε = 0.5 px).
    ///
    /// Returns `true` if the stroke was modified.
    pub fn erase_at(&mut self, center: Pos2, radius: f32) -> bool {
        let radius_sq = radius * radius;
        let mut new_segments: Vec<Vec<Pos2>> = Vec::new();

        for seg in self.segments.drain(..) {
            if seg.len() < 2 {
                continue;
            }

            // --- 1.  Densify to ~1.5 px so that circle-line intersections are
            //         caught between closely-spaced sample points ---
            let pts = densify(&seg, 1.5);

            // --- 2.  Classify every densified point ---
            let inside: Vec<bool> = pts
                .iter()
                .map(|p| (p.x - center.x).powi(2) + (p.y - center.y).powi(2) <= radius_sq)
                .collect();

            // --- 3.  Expand `inside` to cover segments that graze the circle ---
            let mut erase = inside.clone();
            for i in 0..pts.len().saturating_sub(1) {
                if (!erase[i] || !erase[i + 1])
                    && point_to_segment_distance(center, pts[i], pts[i + 1]) <= radius {
                        erase[i] = true;
                        erase[i + 1] = true;
                    }
            }

            // --- 4.  Walk the points, building new segments.  At each
            //         inside↔outside transition, inject the precise
            //         circle-line intersection point. ---
            let mut cur: Vec<Pos2> = Vec::new();

            for i in 0..pts.len() {
                let was_inside = i > 0 && erase[i - 1];
                let is_inside = erase[i];

                if !is_inside {
                    if was_inside {
                        // inside → outside: start new segment with the exit intersection
                        if let Some(ip) = circle_line_intersection(
                            center,
                            radius,
                            pts[i],     // outside
                            pts[i - 1], // inside
                        ) {
                            cur.push(ip);
                        }
                    }
                    cur.push(pts[i]);
                } else {
                    // inside point
                    if !was_inside && i > 0 {
                        // outside → inside: finish current segment with the entry intersection
                        if let Some(ip) = circle_line_intersection(
                            center,
                            radius,
                            pts[i - 1], // outside
                            pts[i],     // inside
                        ) {
                            cur.push(ip);
                        }
                    }
                    // Save the completed segment if long enough
                    if cur.len() >= 2 {
                        new_segments.push(std::mem::take(&mut cur));
                    } else {
                        cur.clear();
                    }
                }
            }
            if cur.len() >= 2 {
                new_segments.push(cur);
            }
        }

        // --- 5.  RDP-simplify (ε = 0.5 px) ---
        for s in &mut new_segments {
            *s = rdp_simplify(std::mem::take(s), 0.5);
        }

        // --- 6.  Clean up ---
        new_segments.retain(|s| s.len() >= 2);
        // Release excess capacity after cutting
        for s in &mut new_segments {
            s.shrink_to_fit();
        }
        new_segments.shrink_to_fit();

        if new_segments != self.segments {
            self.segments = new_segments;
            true
        } else {
            self.segments = new_segments;
            false
        }
    }

    /// Whether this stroke uses highlighter-style rendering.
    #[allow(dead_code)]
    pub fn is_highlighter(&self) -> bool {
        matches!(self.tool_type, AnnotationTool::Highlighter)
    }

    /// Compress all coordinates to 1 decimal place to reduce serialisation
    /// size.  Sub-pixel rendering still looks smooth at this precision.
    pub fn compress(&mut self) {
        for seg in &mut self.segments {
            for p in seg {
                p.x = (p.x * 10.0).round() / 10.0;
                p.y = (p.y * 10.0).round() / 10.0;
            }
        }
    }
}

/// Perpendicular distance from point `p` to line segment `a→b`.
fn point_to_segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let ap = p - a;
    let len_sq = ab.length_sq();
    if len_sq < 1e-6 {
        return ap.length();
    }
    let t = (ap.dot(ab) / len_sq).clamp(0.0, 1.0);
    let proj = a + ab * t;
    (p - proj).length()
}

// ---------------------------------------------------------------------------
// Eraser geometry helpers
// ---------------------------------------------------------------------------

/// Sub-sample a polyline so that consecutive points are no more than
/// `max_step` pixels apart.  Uses a thread-local buffer to avoid
/// repeated allocations on every erase call.
fn densify(points: &[Pos2], max_step: f32) -> Vec<Pos2> {
    TMP_DENSIFY.with(|cell| {
        let mut out = cell.borrow_mut();
        out.clear();
        if points.len() < 2 {
            out.extend_from_slice(points);
            return out.clone();
        }
        out.reserve(points.len() * 2);
        out.push(points[0]);
        for window in points.windows(2) {
            let (a, b) = (window[0], window[1]);
            let dist = a.distance(b);
            if dist > max_step {
                let n = (dist / max_step).ceil() as usize;
                for j in 1..n {
                    let t = j as f32 / n as f32;
                    out.push(Pos2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
                }
            }
            out.push(b);
        }
        out.clone()
    })
}

/// Compute the exact intersection of the directed line segment `from → dir`
/// with the circle centred at `center`.
///
/// Returns the first point (in `[0, 1]` parameter space) where the segment
/// crosses the circle boundary.  Used to inject sub-pixel-precise cut
/// endpoints between inside/outside transitions.
fn circle_line_intersection(center: Pos2, radius: f32, from: Pos2, dir: Pos2) -> Option<Pos2> {
    let d = dir - from;
    let len_sq = d.length_sq();
    if len_sq < 1e-10 {
        return None;
    }
    let f = from - center;
    // Quadratic:  a·t² + b·t + c = 0
    let a = len_sq;
    let b = 2.0 * (f.x * d.x + f.y * d.y);
    let c = f.x * f.x + f.y * f.y - radius * radius;

    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return None;
    }
    let sqrt_d = disc.sqrt();
    let t1 = (-b - sqrt_d) / (2.0 * a);
    let t2 = (-b + sqrt_d) / (2.0 * a);

    let from_inside = f.x * f.x + f.y * f.y <= radius * radius;

    let t = if from_inside {
        // Inside → outside: pick the larger root as the exit.
        if (0.0..=1.0).contains(&t1) {
            t1
        } else {
            t2
        }
    } else {
        // Outside → inside: pick the smaller root as the entry.
        if (0.0..=1.0).contains(&t2) {
            t2
        } else {
            t1
        }
    };

    if (0.0..=1.0).contains(&t) {
        Some(Pos2::new(from.x + d.x * t, from.y + d.y * t))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// RDP simplification
// ---------------------------------------------------------------------------

/// Ramer–Douglas–Peucker polyline simplification.
///
/// Keeps the endpoints and recursively discards intermediate points whose
/// perpendicular distance to the line is below `epsilon`.
fn rdp_simplify(points: Vec<Pos2>, epsilon: f32) -> Vec<Pos2> {
    if points.len() < 3 {
        return points;
    }
    let epsilon_sq = epsilon * epsilon;

    // Find the point farthest from the line (first, last)
    let first = points[0];
    let last = points[points.len() - 1];
    let mut dmax_sq = 0.0_f32;
    let mut index = 0;

    for (i, &p) in points.iter().enumerate().skip(1).take(points.len() - 1) {
        let d_sq = perpendicular_distance_sq(p, first, last);
        if d_sq > dmax_sq {
            dmax_sq = d_sq;
            index = i;
        }
    }

    if dmax_sq > epsilon_sq {
        let mut left = rdp_simplify(points[..=index].to_vec(), epsilon);
        let right = rdp_simplify(points[index..].to_vec(), epsilon);
        left.pop(); // remove duplicate at split point
        left.extend(right);
        left
    } else {
        vec![first, last]
    }
}

fn perpendicular_distance_sq(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_sq();
    if len_sq < 1e-10 {
        return (p - a).length_sq();
    }
    let t = ((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len_sq;
    let proj = Pos2::new(a.x + t * ab.x, a.y + t * ab.y);
    (p - proj).length_sq()
}

/// Linearly interpolate points between `prev` and `curr` with `step` spacing.
/// Used to fill gaps in the eraser path when the cursor moves faster than the
/// frame rate can sample.
fn interpolate_eraser_path(prev: Option<Pos2>, curr: Option<Pos2>, step: f32) -> Vec<Pos2> {
    let Some(curr) = curr else {
        return Vec::new();
    };
    let Some(prev) = prev else {
        return vec![curr];
    };
    let dist = curr.distance(prev);
    if dist <= step {
        return vec![curr];
    }
    let n = (dist / step).ceil() as usize;
    let mut points = Vec::with_capacity(n);
    for i in 0..n {
        let t = (i + 1) as f32 / n as f32;
        points.push(egui::pos2(
            prev.x + (curr.x - prev.x) * t,
            prev.y + (curr.y - prev.y) * t,
        ));
    }
    points
}

// ---------------------------------------------------------------------------
// Annotation state
// ---------------------------------------------------------------------------

/// Fixed palette: red, yellow, orange, blue, green.
pub const ANNOTATION_PALETTE: &[Color32] = &[
    Color32::from_rgb(0xE5, 0x30, 0x2B), // red
    Color32::from_rgb(0xFF, 0xC1, 0x07), // yellow
    Color32::from_rgb(0xFF, 0x85, 0x00), // orange
    Color32::from_rgb(0x1E, 0x88, 0xE5), // blue
    Color32::from_rgb(0x43, 0xA0, 0x47), // green
];

pub const PEN_THICKNESS_FINE: f32 = 2.5;
pub const PEN_THICKNESS_MEDIUM: f32 = 5.0;
pub const PEN_THICKNESS_THICK: f32 = 9.0;

pub const HIGHLIGHTER_THICKNESS: f32 = 18.0;
pub const HIGHLIGHTER_ALPHA: u8 = 110; // ~43% opacity

pub const ERASER_DEFAULT_SIZE: f32 = 20.0;
pub const ERASER_MIN_SIZE: f32 = 2.0;
pub const ERASER_MAX_SIZE: f32 = 100.0;

/// Hard limit: max number of strokes per page.
pub const MAX_STROKES: usize = 2000;
/// Hard limit: max total vertices across all strokes per page (~800KB at 8 bytes/point).
pub const MAX_TOTAL_POINTS: usize = 100_000;

// ---------------------------------------------------------------------------
// Eraser size preset
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EraserPreset {
    Small,  // 8px
    Medium, // 20px (default)
    Large,  // 40px
}

impl EraserPreset {
    pub fn size(self) -> f32 {
        match self {
            EraserPreset::Small => 8.0,
            EraserPreset::Medium => 20.0,
            EraserPreset::Large => 40.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            EraserPreset::Small => "小",
            EraserPreset::Medium => "中",
            EraserPreset::Large => "大",
        }
    }
}

// ---------------------------------------------------------------------------
// Pen thickness
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PenThickness {
    Fine,
    Medium,
    Thick,
}

impl PenThickness {
    pub fn value(self) -> f32 {
        match self {
            PenThickness::Fine => PEN_THICKNESS_FINE,
            PenThickness::Medium => PEN_THICKNESS_MEDIUM,
            PenThickness::Thick => PEN_THICKNESS_THICK,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnnotationState {
    pub current_tool: AnnotationTool,
    pub current_color: Color32,
    pub pen_thickness: PenThickness,
    pub eraser_size: f32,
    pub eraser_size_preset: EraserPreset,
    /// Show the eraser adjustment panel.
    pub show_eraser_panel: bool,
    pub strokes: Vec<StrokeData>,

    /// Stroke being drawn this frame.
    active_stroke: Option<StrokeData>,

    /// Current cursor position on the canvas (screen-space).
    pub cursor_pos: Option<Pos2>,

    /// Previous frame cursor position — used for path interpolation during erase.
    last_cursor_pos: Option<Pos2>,

    /// True if we are currently erasing.
    pub erasing: bool,
}

impl Default for AnnotationState {
    fn default() -> Self {
        Self {
            current_tool: AnnotationTool::Pen,
            current_color: ANNOTATION_PALETTE[0],
            pen_thickness: PenThickness::Medium,
            eraser_size: ERASER_DEFAULT_SIZE,
            eraser_size_preset: EraserPreset::Medium,
            show_eraser_panel: false,
            strokes: Vec::new(),
            active_stroke: None,
            cursor_pos: None,
            last_cursor_pos: None,
            erasing: false,
        }
    }
}

impl AnnotationState {
    pub fn new() -> Self {
        Self::default()
    }

    // ------------------------------------------------------------------
    // Tool / color selection
    // ------------------------------------------------------------------

    pub fn set_tool(&mut self, tool: AnnotationTool) {
        self.current_tool = tool;
        self.active_stroke = None;
        // Auto-show eraser panel when eraser is selected
        self.show_eraser_panel = matches!(tool, AnnotationTool::Eraser);
    }

    pub fn set_eraser_size(&mut self, size: f32) {
        self.eraser_size = size.clamp(ERASER_MIN_SIZE, ERASER_MAX_SIZE);
        // Snap to nearest preset
        self.eraser_size_preset = if self.eraser_size <= 14.0 {
            EraserPreset::Small
        } else if self.eraser_size <= 30.0 {
            EraserPreset::Medium
        } else {
            EraserPreset::Large
        };
        self.show_eraser_panel = true;
    }

    pub fn set_eraser_preset(&mut self, preset: EraserPreset) {
        self.eraser_size_preset = preset;
        self.eraser_size = preset.size();
        self.show_eraser_panel = true;
    }

    /// Adjust eraser size by delta (positive = larger, negative = smaller).
    pub fn adjust_eraser_size(&mut self, delta: f32) {
        let new = self.eraser_size + delta;
        self.set_eraser_size(new);
    }

    pub fn set_color(&mut self, color: Color32) {
        self.current_color = color;
    }

    #[allow(dead_code)]
    pub fn cycle_thickness(&mut self) {
        self.pen_thickness = match self.pen_thickness {
            PenThickness::Fine => PenThickness::Medium,
            PenThickness::Medium => PenThickness::Thick,
            PenThickness::Thick => PenThickness::Fine,
        };
    }

    /// Clear all strokes and the active stroke-in-progress.
    pub fn clear_screen(&mut self) {
        self.strokes.clear();
        self.active_stroke = None;
    }

    /// Set strokes directly (for loading saved annotations).
    pub fn set_strokes(&mut self, strokes: Vec<StrokeData>) {
        self.strokes = strokes;
        self.active_stroke = None;
    }

    pub fn undo(&mut self) {
        self.strokes.pop();
    }

    // ------------------------------------------------------------------
    // Input handling
    // ------------------------------------------------------------------

    /// Update state from mouse / touch input.
    pub fn handle_input(&mut self, ctx: &egui::Context, response: &egui::Response) {
        let hover = response.hover_pos();
        self.last_cursor_pos = self.cursor_pos;
        self.cursor_pos = hover;

        // --- Detect eraser triggers ---
        let right_clicked = ctx.input(|i| i.pointer.secondary_down());

        // Two-finger touch: check if it's a pinch (resize) or erase
        let two_finger_touch = ctx.input(|i| {
            i.multi_touch()
                .map(|mt| mt.num_touches >= 2)
                .unwrap_or(false)
        });
        let two_finger_center = ctx.input(|i| i.multi_touch().map(|mt| mt.start_pos));
        let zoom_delta = ctx.input(|i| i.multi_touch().map(|mt| mt.zoom_delta).unwrap_or(1.0));

        // Pinch resize: when two fingers are touching but the zoom delta
        // indicates a pinch gesture (not just two fingers held still).
        if matches!(self.current_tool, AnnotationTool::Eraser)
            && two_finger_touch && (zoom_delta - 1.0).abs() > 0.02 {
                let delta = (zoom_delta - 1.0) * self.eraser_size * 0.5;
                self.adjust_eraser_size(delta);
                return; // don't erase during pinch
            }

        let wants_erase = matches!(self.current_tool, AnnotationTool::Eraser)
            || right_clicked
            || two_finger_touch;

        self.erasing = wants_erase;

        // --- Pen / Highlighter drawing ---
        match self.current_tool {
            AnnotationTool::Pen | AnnotationTool::Highlighter
                if !self.erasing => {
                    if response.drag_started() {
                        if let Some(p) = hover {
                            self.start_stroke(p);
                        }
                    }
                    if response.dragged() {
                        if let Some(p) = hover {
                            self.extend_stroke(p);
                        }
                    }
                    if response.drag_stopped() {
                        self.commit_stroke();
                    }
                }
            _ => {}
        }

        // --- Erasing (with interpolation for high precision) ---
        if self.erasing {
            // Build the eraser path: interpolate between previous and current cursor
            let effective_radius = self.eraser_size + 1.0; // AA compensation
            let step = (effective_radius * 0.5).max(1.0); // sample density = radius/2

            let eraser_points =
                interpolate_eraser_path(self.last_cursor_pos, self.cursor_pos, step);

            if right_clicked || matches!(self.current_tool, AnnotationTool::Eraser) {
                for p in &eraser_points {
                    self.erase_at(*p, effective_radius);
                }
            }
            // Two-finger touch: erase at the touch center (no interpolation needed)
            if two_finger_touch {
                if let Some(p) = two_finger_center {
                    self.erase_at(p, effective_radius);
                }
            }
        }
    }

    fn start_stroke(&mut self, p: Pos2) {
        let (color, thickness, tool_type) = match self.current_tool {
            AnnotationTool::Pen => (
                self.current_color,
                self.pen_thickness.value(),
                AnnotationTool::Pen,
            ),
            AnnotationTool::Highlighter => {
                let mut c = self.current_color;
                c = Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), HIGHLIGHTER_ALPHA);
                (c, HIGHLIGHTER_THICKNESS, AnnotationTool::Highlighter)
            }
            _ => return,
        };
        let mut s = StrokeData::new(color, thickness, tool_type);
        s.push_point(p);
        self.active_stroke = Some(s);
    }

    fn extend_stroke(&mut self, p: Pos2) {
        if let Some(ref mut s) = self.active_stroke {
            if let Some(&last) = s.last_point() {
                if (p - last).length() < 0.5 {
                    return;
                }
            }
            s.push_point(p);
        }
    }

    fn commit_stroke(&mut self) {
        if let Some(s) = self.active_stroke.take() {
            if s.valid_segment_count() > 0 || s.total_points() >= 2 {
                self.strokes.push(s);
                // Enforce stroke cap — drop oldest when over limit
                while self.strokes.len() > MAX_STROKES {
                    self.strokes.remove(0);
                }
                // Enforce total vertex cap — trim from oldest strokes
                self.prune_excess_points();
            }
        }
    }

    /// Remove points from the oldest strokes until total vertex count
    /// is within `MAX_TOTAL_POINTS`.
    fn prune_excess_points(&mut self) {
        let total: usize = self.strokes.iter().map(|s| s.total_points()).sum();
        if total <= MAX_TOTAL_POINTS {
            return;
        }
        // Pop entire oldest strokes until we are under the cap
        // (finer-grained per-point pruning would be more complex for marginal gain)
        while !self.strokes.is_empty() {
            let pts: usize = self.strokes.iter().map(|s| s.total_points()).sum();
            if pts <= MAX_TOTAL_POINTS {
                break;
            }
            self.strokes.remove(0);
        }
    }

    fn erase_at(&mut self, p: Pos2, radius: f32) {
        // Use segment-based cut: each segment is split at erase boundaries.
        // Strokes with zero valid segments after cutting are removed.
        self.strokes.retain_mut(|s| {
            let modified = s.erase_at(p, radius);
            // Keep the stroke if it still has any segments with ≥2 points
            if modified {
                s.valid_segment_count() > 0
            } else {
                true
            }
        });
    }

    // ------------------------------------------------------------------
    // Rendering
    // ------------------------------------------------------------------

    /// Paint all strokes (and the in-progress active stroke) onto the canvas.
    pub fn paint(&self, painter: &egui::Painter) {
        for stroke in &self.strokes {
            paint_stroke(painter, stroke);
        }
        if let Some(ref s) = self.active_stroke {
            paint_stroke(painter, s);
        }
    }

    /// Draw the eraser cursor as a semi-transparent circle.
    pub fn paint_eraser_cursor(&self, painter: &egui::Painter) {
        if let Some(p) = self.cursor_pos {
            if self.erasing || matches!(self.current_tool, AnnotationTool::Eraser) {
                let fill = Color32::from_rgba_unmultiplied(128, 128, 128, 60);
                let stroke =
                    egui::Stroke::new(2.0_f32, Color32::from_rgba_unmultiplied(80, 80, 80, 120));
                painter.circle_filled(p, self.eraser_size, fill);
                painter.circle_stroke(p, self.eraser_size, stroke);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Stroke rendering
// ---------------------------------------------------------------------------

fn paint_stroke(painter: &egui::Painter, stroke: &StrokeData) {
    let stroke_style = egui::Stroke::new(stroke.thickness, stroke.color);
    for segment in &stroke.segments {
        for window in segment.windows(2) {
            painter.line_segment([window[0], window[1]], stroke_style);
        }
    }
}
