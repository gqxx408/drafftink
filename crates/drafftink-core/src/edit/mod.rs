//! Edit mode — lightweight element selection, drag, resize, and property
//! editing.  All state lives in `EditState` so that switching back to
//! Display mode drops everything cleanly.

use std::collections::{HashMap, HashSet};
use egui::Pos2;
use uuid::Uuid;

// Sub-modules
pub mod selection;
pub mod inspector;

// ---------------------------------------------------------------------------
// AppMode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Display,
    Edit,
}

// ---------------------------------------------------------------------------
// EditState — single entry-point for all Edit-mode mutable state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct EditState {
    /// Currently selected element IDs (supports multi-select via Shift+click
    /// or marquee).
    pub selected: HashSet<Uuid>,

    /// Marquee-selection start in **screen** space.
    pub marquee_start: Option<Pos2>,

    /// Marquee-selection end in **screen** space.
    pub marquee_end: Option<Pos2>,

    /// Dash offset for the selection border animation.
    /// Incremented by 1.0 every frame in Edit mode.
    pub dash_offset: f32,

    /// Element(s) being dragged.  `None` when idle.
    pub dragging: Option<DragState>,

    /// Resize handle being dragged, if any.
    pub resizing: Option<ResizeState>,

    /// Which resize handle is under the cursor (for cursor icon change).
    pub hovered_handle: Option<ResizeHandle>,

    /// `true` during animation preview (temporarily re-enters Display-like
    /// playback while keeping Edit mode UI visible).
    pub previewing: bool,
}

// ---------------------------------------------------------------------------
// Drag / Resize helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DragState {
    /// IDs of all elements being moved (supports multi-select drag).
    pub element_ids: HashSet<Uuid>,
    /// World-space positions recorded at the instant the drag began.
    pub start_positions: HashMap<Uuid, [f32; 2]>,
    /// Mouse position (screen-space) when drag began.
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
    TopLeft,    TopCenter,    TopRight,
    MidLeft,                   MidRight,
    BottomLeft, BottomCenter,  BottomRight,
}
