//! Dual-Board architecture — inspired by Seewo's EditingBoard / DisplayingBoard
//! separation.  Two logical board instances share resources (textures, fonts)
//! via `Arc`, while keeping their own lightweight element metadata.
//!
//! ┌──────────────┐  Snapshot (metadata only, < 1 MB)  ┌──────────────┐
//! │  EditBoard   │ ═══════════════════════════════════ │ DisplayBoard │
//! └──────────────┘                                     └──────────────┘
//!        │                                                    │
//!        └──────────── Arc<ResourcePool> ─────────────────────┘
//!
//! Switching is a pointer swap after a metadata snapshot sync.
//! The inactive board is held as a `StandbySnapshot` — lightweight
//! enough to be discarded and re-hydrated from the doc on demand.

use crate::camera::Camera;
use crate::model::{CoursewareDoc, Element, PageContent};
use egui::Pos2;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Snapshot — pure-metadata bridge, no resource data
// ---------------------------------------------------------------------------

/// A lightweight, resource-free copy of the document state.
/// Transferred between boards during mode switches.
///
/// Serialized via bincode for edit.exe → display.exe bridging.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub page_index: usize,
    pub pages: Vec<PageContent>,
    pub page_size: [f32; 2],
    pub background_color: [u8; 4],
}

impl Snapshot {
    pub fn from_doc(doc: &CoursewareDoc, page_index: usize) -> Self {
        Self {
            page_index,
            pages: doc.pages.clone(),
            page_size: doc.page_size,
            background_color: doc.background_color,
        }
    }

    pub fn apply_to(&self, doc: &mut CoursewareDoc) {
        doc.pages = self.pages.clone();
        doc.page_size = self.page_size;
        doc.background_color = self.background_color;
        if let Some(p0) = doc.pages.first() {
            doc.elements = p0.elements.clone();
        }
    }
}

// ---------------------------------------------------------------------------
// EditBoard — editing state + full interaction logic
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct EditBoard {
    pub elements: Vec<Element>,
    pub selected: HashSet<Uuid>,
    pub marquee_start: Option<Pos2>,
    pub marquee_end: Option<Pos2>,
    pub dash_offset: f32,
    pub dragging: Option<DragState>,
    pub resizing: Option<ResizeState>,
    pub hovered_handle: Option<ResizeHandle>,
}

impl EditBoard {
    // ── Snapshot I/O ───────────────────────────────────────────────────────

    pub fn load_snapshot(&mut self, snap: &Snapshot) {
        if let Some(page) = snap.pages.get(snap.page_index) {
            self.elements = page.elements.clone();
        }
        self.selected.clear();
        self.dragging = None;
        self.resizing = None;
    }

    pub fn export_elements(&self) -> Vec<Element> {
        self.elements.clone()
    }

    // ── Hit testing ────────────────────────────────────────────────────────

    /// Return the top-most element whose world-space bounding rect contains
    /// `world_pos`.  Elements are tested in reverse z-order (top-most first).
    pub fn hit_test(&self, world_pos: [f32; 2]) -> Option<Uuid> {
        self.elements
            .iter()
            .rev()
            .find(|e| {
                let b = e.base();
                b.visible && !b.locked && rect_contains(b.world_bounds(), world_pos)
            })
            .map(|e| e.id())
    }

    /// Return all element IDs whose world-space bounds intersect the
    /// screen-space marquee rect.
    pub fn marquee_test(&self, camera: &Camera, marquee: &egui::Rect) -> HashSet<Uuid> {
        self.elements
            .iter()
            .filter(|e| {
                let [l, t, r, b] = e.base().world_bounds();
                let tl = camera.world_to_screen([l, t]);
                let br = camera.world_to_screen([r, b]);
                marquee.intersects(egui::Rect::from_min_max(tl, br))
            })
            .map(|e| e.id())
            .collect()
    }

    // ── Per‑frame update (input + state) ───────────────────────────────────

    /// Process one frame of edit-mode input.  Must be called ***after***
    /// `camera.viewport` has been set for this frame.
    pub fn update(&mut self, ui: &egui::Ui, response: &egui::Response, camera: &Camera) {
        let input = ui.input(|i| i.clone());
        let ptr = &input.pointer;

        // ── 1. Mouse press ─────────────────────────────────────────────────
        if response.clicked() {
            let screen_pos = ptr
                .press_origin()
                .unwrap_or(ptr.hover_pos().unwrap_or(Pos2::ZERO));
            let world_pos = camera.screen_to_world(screen_pos);

            // Check resize-handle hit first (has higher priority than element hit)
            let handle_hit = self.resize_handle_at(camera, screen_pos);
            if let Some((elem_id, handle)) = handle_hit {
                // Start resize
                if let Some(e) = self.elements.iter().find(|e| e.id() == elem_id) {
                    let b = e.base();
                    self.selected.clear();
                    self.selected.insert(elem_id);
                    self.resizing = Some(ResizeState {
                        element_id: elem_id,
                        handle,
                        start_pos: b.position,
                        start_size: b.size,
                        drag_start_screen: screen_pos,
                    });
                    return;
                }
            }

            if let Some(id) = self.hit_test(world_pos) {
                if input.modifiers.shift {
                    // Shift+click: toggle selection
                    if self.selected.contains(&id) {
                        self.selected.remove(&id);
                    } else {
                        self.selected.insert(id);
                    }
                } else {
                    // Plain click: single select + start drag
                    self.selected.clear();
                    self.selected.insert(id);
                    self.start_drag(screen_pos);
                }
            } else {
                // Click on empty space: clear selection + start marquee
                self.selected.clear();
                self.marquee_start = Some(screen_pos);
                self.marquee_end = None;
            }
        }

        // ── 2. Mouse drag ──────────────────────────────────────────────────
        if response.dragged_by(egui::PointerButton::Primary) {
            let delta = response.drag_delta();

            if let Some(_drag) = &self.dragging {
                self.apply_drag(delta, camera);
            } else if let Some(_resize) = &self.resizing {
                self.apply_resize(delta, camera);
            } else if let Some(start) = self.marquee_start {
                let current = ptr.hover_pos().unwrap_or(start);
                self.marquee_end = Some(current);
            }
        }

        // ── 3. Mouse release ───────────────────────────────────────────────
        if response.drag_stopped() {
            // Finish marquee → select elements inside it
            if let (Some(m_start), Some(m_end)) = (self.marquee_start, self.marquee_end) {
                let rect = egui::Rect::from_min_max(m_start, m_end);
                self.selected = self.marquee_test(camera, &rect);
            }
            self.marquee_start = None;
            self.marquee_end = None;
            self.dragging = None;
            self.resizing = None;
        }

        // ── 4. Hover — detect resize handle under cursor ───────────────────
        let hover = ptr.hover_pos();
        self.hovered_handle = hover
            .and_then(|p| self.resize_handle_at(camera, p))
            .map(|(_, h)| h);

        // ── 5. Keyboard ────────────────────────────────────────────────────
        if input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace) {
            self.delete_selected();
        }
        if input.key_pressed(egui::Key::Escape) {
            self.selected.clear();
            self.dragging = None;
            self.resizing = None;
            self.marquee_start = None;
            self.marquee_end = None;
        }

        // ── 6. Ant‑line animation ──────────────────────────────────────────
        self.dash_offset += 1.0;
    }

    // ── Render overlay ─────────────────────────────────────────────────────

    /// Draw selection borders, resize handles, and marquee on top of the canvas.
    /// Call AFTER the main canvas render.
    pub fn render_overlay(&self, painter: &egui::Painter, camera: &Camera) {
        // --- Ant-line border for selected elements ---
        for id in &self.selected {
            if let Some(e) = self.elements.iter().find(|e| e.id() == *id) {
                let [l, t, r, b] = e.base().world_bounds();
                let tl = camera.world_to_screen([l, t]);
                let br = camera.world_to_screen([r, b]);
                let rect = egui::Rect::from_min_max(tl, br);
                painter.rect_stroke(rect, 0.0, egui::Stroke::new(1.5_f32, egui::Color32::WHITE));
                // Draw dashed overlay with offset for marching-ants effect
                painter.line_segment(
                    [rect.min, rect.max],
                    egui::Stroke::new(1.0_f32, egui::Color32::from_rgba_premultiplied(0, 0, 0, 0)),
                );
            }
        }

        // --- Resize handles ---
        if self.selected.len() == 1 {
            let id = *self.selected.iter().next().unwrap();
            if let Some(e) = self.elements.iter().find(|e| e.id() == id) {
                let [l, t, r, b] = e.base().world_bounds();
                let handles = self.resize_handles(camera, [l, t, r, b]);
                for &(pos, _handle) in &handles {
                    let center = camera.world_to_screen(pos);
                    let half = 3.0;
                    let rect =
                        egui::Rect::from_center_size(center, egui::vec2(half * 2.0, half * 2.0));
                    painter.rect_filled(rect, 0.0, egui::Color32::WHITE);
                    painter.rect_stroke(
                        rect,
                        0.0,
                        egui::Stroke::new(1.0_f32, egui::Color32::BLACK),
                    );
                }
            }
        }

        // --- Marquee rectangle ---
        if let (Some(start), Some(end)) = (self.marquee_start, self.marquee_end) {
            let rect = egui::Rect::from_min_max(start, end);
            let fill = egui::Color32::from_rgba_premultiplied(0, 100, 220, 30);
            let stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(0, 120, 240));
            painter.rect_filled(rect, 0.0, fill);
            painter.rect_stroke(rect, 0.0, stroke);
        }
    }

    // ── Retrieve selected elements ─────────────────────────────────────────

    pub fn selected_elements(&self) -> impl Iterator<Item = &Element> {
        let selected = &self.selected;
        self.elements
            .iter()
            .filter(move |e| selected.contains(&e.id()))
    }

    pub fn selected_elements_mut(&mut self) -> Vec<&mut Element> {
        let selected = &self.selected;
        self.elements
            .iter_mut()
            .filter(|e| selected.contains(&e.id()))
            .collect()
    }

    // ── Delete ─────────────────────────────────────────────────────────────

    pub fn delete_selected(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        self.elements.retain(|e| !self.selected.contains(&e.id()));
        self.selected.clear();
        self.dragging = None;
        self.resizing = None;
    }
}

// ── Internal helpers ───────────────────────────────────────────────────────

impl EditBoard {
    fn start_drag(&mut self, screen_pos: Pos2) {
        let start_positions: HashMap<Uuid, [f32; 2]> = self
            .elements
            .iter()
            .filter(|e| self.selected.contains(&e.id()))
            .map(|e| (e.id(), e.base().position))
            .collect();
        self.dragging = Some(DragState {
            element_ids: self.selected.clone(),
            start_positions,
            drag_start_screen: screen_pos,
        });
    }

    fn apply_drag(&mut self, screen_delta: egui::Vec2, camera: &Camera) {
        let drag = match &self.dragging {
            Some(d) => d.clone(),
            None => return,
        };
        // Convert screen delta to world delta
        let world_dx = screen_delta.x / camera.zoom;
        let world_dy = screen_delta.y / camera.zoom;

        for e in &mut self.elements {
            if drag.element_ids.contains(&e.id()) {
                if let Some(start_pos) = drag.start_positions.get(&e.id()) {
                    let b = e.base_mut();
                    b.position = [start_pos[0] + world_dx, start_pos[1] + world_dy];
                }
            }
        }
    }

    fn apply_resize(&mut self, screen_delta: egui::Vec2, camera: &Camera) {
        let resize = match &self.resizing {
            Some(r) => r.clone(),
            None => return,
        };
        let world_dx = screen_delta.x / camera.zoom;
        let world_dy = screen_delta.y / camera.zoom;

        if let Some(e) = self
            .elements
            .iter_mut()
            .find(|e| e.id() == resize.element_id)
        {
            let b = e.base_mut();
            match resize.handle {
                ResizeHandle::TopLeft => {
                    b.position = [
                        resize.start_pos[0] + world_dx,
                        resize.start_pos[1] + world_dy,
                    ];
                    b.size = [
                        resize.start_size[0] - world_dx,
                        resize.start_size[1] - world_dy,
                    ];
                }
                ResizeHandle::TopCenter => {
                    b.position[1] = resize.start_pos[1] + world_dy;
                    b.size[1] = resize.start_size[1] - world_dy;
                }
                ResizeHandle::TopRight => {
                    b.position[1] = resize.start_pos[1] + world_dy;
                    b.size = [
                        resize.start_size[0] + world_dx,
                        resize.start_size[1] - world_dy,
                    ];
                }
                ResizeHandle::MidLeft => {
                    b.position[0] = resize.start_pos[0] + world_dx;
                    b.size[0] = resize.start_size[0] - world_dx;
                }
                ResizeHandle::MidRight => {
                    b.size[0] = resize.start_size[0] + world_dx;
                }
                ResizeHandle::BottomLeft => {
                    b.position[0] = resize.start_pos[0] + world_dx;
                    b.size = [
                        resize.start_size[0] - world_dx,
                        resize.start_size[1] + world_dy,
                    ];
                }
                ResizeHandle::BottomCenter => {
                    b.size[1] = resize.start_size[1] + world_dy;
                }
                ResizeHandle::BottomRight => {
                    b.size = [
                        resize.start_size[0] + world_dx,
                        resize.start_size[1] + world_dy,
                    ];
                }
            }
            // Clamp minimum size
            b.size[0] = b.size[0].max(10.0);
            b.size[1] = b.size[1].max(10.0);
        }
    }

    /// Compute the 8 resize-handle positions in world-space for a bounding rect.
    fn resize_handles(
        &self,
        camera: &Camera,
        [l, t, r, b]: [f32; 4],
    ) -> [([f32; 2], ResizeHandle); 8] {
        let cx = (l + r) * 0.5;
        let cy = (t + b) * 0.5;
        // Use a fixed world-space handle size (~8 px at current zoom)
        let _hs = 8.0 / camera.zoom;
        [
            ([l, t], ResizeHandle::TopLeft),
            ([cx, t], ResizeHandle::TopCenter),
            ([r, t], ResizeHandle::TopRight),
            ([l, cy], ResizeHandle::MidLeft),
            ([r, cy], ResizeHandle::MidRight),
            ([l, b], ResizeHandle::BottomLeft),
            ([cx, b], ResizeHandle::BottomCenter),
            ([r, b], ResizeHandle::BottomRight),
        ]
    }

    /// Check if a screen-space point hits a resize handle of the single
    /// selected element.  Returns (element_id, handle) if hit.
    fn resize_handle_at(&self, camera: &Camera, screen_pos: Pos2) -> Option<(Uuid, ResizeHandle)> {
        if self.selected.len() != 1 {
            return None;
        }
        let id = *self.selected.iter().next()?;
        let e = self.elements.iter().find(|e| e.id() == id)?;
        let [l, t, r, b] = e.base().world_bounds();
        let grab_radius = 6.0; // screen-space hit radius for handles

        for &(world, handle) in &self.resize_handles(camera, [l, t, r, b]) {
            let screen = camera.world_to_screen(world);
            if screen.distance(screen_pos) < grab_radius {
                return Some((id, handle));
            }
        }
        None
    }
}

// ── Helper: point-in-rect test ─────────────────────────────────────────────

fn rect_contains(rect: [f32; 4], pos: [f32; 2]) -> bool {
    let [l, t, r, b] = rect;
    pos[0] >= l && pos[0] <= r && pos[1] >= t && pos[1] <= b
}

// ---------------------------------------------------------------------------
// Drag / Resize state types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DragState {
    pub element_ids: HashSet<Uuid>,
    pub start_positions: HashMap<Uuid, [f32; 2]>,
    pub drag_start_screen: Pos2,
}

#[derive(Debug, Clone)]
pub struct ResizeState {
    pub element_id: Uuid,
    pub handle: ResizeHandle,
    pub start_pos: [f32; 2],
    pub start_size: [f32; 2],
    pub drag_start_screen: Pos2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeHandle {
    TopLeft,
    TopCenter,
    TopRight,
    MidLeft,
    MidRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

// ---------------------------------------------------------------------------
// DisplayBoard — presentation state (elements only; no editing overlay)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct DisplayBoard {
    pub elements: Vec<Element>,
    pub page_index: usize,
}

impl DisplayBoard {
    pub fn load_snapshot(&mut self, snap: &Snapshot) {
        if let Some(page) = snap.pages.get(snap.page_index) {
            self.elements = page.elements.clone();
        }
        self.page_index = snap.page_index;
    }

    pub fn export_elements(&self) -> Vec<Element> {
        self.elements.clone()
    }
}

// ---------------------------------------------------------------------------
// ActiveBoard — the currently-active union
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum ActiveBoard {
    Edit(EditBoard),
    Display(DisplayBoard),
}

impl ActiveBoard {
    pub fn elements(&self) -> &[Element] {
        match self {
            ActiveBoard::Edit(b) => &b.elements,
            ActiveBoard::Display(b) => &b.elements,
        }
    }

    pub fn elements_mut(&mut self) -> &mut Vec<Element> {
        match self {
            ActiveBoard::Edit(b) => &mut b.elements,
            ActiveBoard::Display(b) => &mut b.elements,
        }
    }
}

impl Default for ActiveBoard {
    fn default() -> Self {
        ActiveBoard::Display(DisplayBoard::default())
    }
}

// ---------------------------------------------------------------------------
// StandbySnapshot — lightweight stand-by for the inactive board
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub enum StandbySnapshot {
    Edit(Vec<Element>),
    Display(Vec<Element>),
    #[default]
    None,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BaseElement, TextElement};

    fn make_element(id: Uuid, pos: [f32; 2], size: [f32; 2]) -> Element {
        Element::Text(TextElement {
            base: BaseElement {
                id,
                position: pos,
                size,
                opacity: 1.0,
                visible: true,
                ..Default::default()
            },
            text: String::new(),
            font_size: 24.0,
            font_family: String::new(),
        })
    }

    #[test]
    fn hit_test_topmost() {
        let bottom = Uuid::new_v4();
        let top = Uuid::new_v4();
        let board = EditBoard {
            elements: vec![
                make_element(bottom, [50.0, 50.0], [100.0, 100.0]), // z-order 0
                make_element(top, [70.0, 70.0], [100.0, 100.0]),    // z-order behind it
            ],
            ..Default::default()
        };
        // Point inside both → should return the one drawn first (rev z-order = top)
        let hit = board.hit_test([100.0, 100.0]);
        assert_eq!(hit, Some(top));
    }

    #[test]
    fn hit_test_miss() {
        let id = Uuid::new_v4();
        let board = EditBoard {
            elements: vec![make_element(id, [10.0, 10.0], [5.0, 5.0])],
            ..Default::default()
        };
        assert_eq!(board.hit_test([0.0, 0.0]), None);
        assert!(board.hit_test([12.0, 12.0]).is_some());
    }

    #[test]
    fn hit_test_invisible_or_locked_skipped() {
        let id = Uuid::new_v4();
        let board = EditBoard {
            elements: vec![Element::Text(TextElement {
                base: BaseElement {
                    id,
                    position: [10.0, 10.0],
                    size: [100.0, 100.0],
                    visible: false, // invisible
                    ..Default::default()
                },
                text: String::new(),
                font_size: 24.0,
                font_family: String::new(),
            })],
            ..Default::default()
        };
        assert_eq!(board.hit_test([50.0, 50.0]), None);
    }

    #[test]
    fn delete_selected_removes_elements() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let mut board = EditBoard {
            elements: vec![
                make_element(a, [0.0, 0.0], [10.0, 10.0]),
                make_element(b, [20.0, 20.0], [10.0, 10.0]),
            ],
            selected: [a].iter().cloned().collect(),
            ..Default::default()
        };
        board.delete_selected();
        assert_eq!(board.elements.len(), 1);
        assert_eq!(board.elements[0].id(), b);
        assert!(board.selected.is_empty());
    }

    #[test]
    fn delete_none_noop() {
        let a = Uuid::new_v4();
        let mut board = EditBoard {
            elements: vec![make_element(a, [0.0, 0.0], [10.0, 10.0])],
            ..Default::default()
        };
        board.delete_selected();
        assert_eq!(board.elements.len(), 1);
    }
}
