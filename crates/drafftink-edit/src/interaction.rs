//! Interaction state machine — tracks tool mode, selection, drag state,
//! and handles mouse/keyboard events on the canvas.
#![allow(dead_code)]
use drafftink_core::model::{ElementId, ShapeType};
use egui::Pos2;

// ---------------------------------------------------------------------------
// Tool mode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ToolMode {
    #[default]
    Select,
    DrawShape(ShapeType),
    DrawPath,
    Text,
    Image,
    Pan,
}

impl ToolMode {
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            ToolMode::Select => "Select",
            ToolMode::DrawShape(ShapeType::Rectangle) => "Rectangle",
            ToolMode::DrawShape(ShapeType::Ellipse) => "Ellipse",
            ToolMode::DrawShape(ShapeType::Line) => "Line",
            ToolMode::DrawShape(ShapeType::Arrow) => "Arrow",
            ToolMode::DrawShape(ShapeType::Bracket) => "Bracket",
            ToolMode::DrawShape(ShapeType::Brace) => "Brace",
            ToolMode::DrawShape(ShapeType::Fan) => "Fan",
            ToolMode::DrawPath => "Pen",
            ToolMode::Text => "Text",
            ToolMode::Image => "Image",
            ToolMode::Pan => "Pan",
        }
    }
}

// ---------------------------------------------------------------------------
// Interaction state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct InteractionState {
    /// Currently active tool.
    pub mode: ToolMode,

    /// Currently selected element ids (ordered by selection time).
    pub selected_ids: Vec<ElementId>,

    /// Element currently under the cursor.
    #[allow(dead_code)]
    pub hovered_id: Option<ElementId>,

    /// Whether a drag operation is in progress.
    pub is_dragging: bool,

    /// World-space position where the drag started.
    pub drag_start_world: Option<[f32; 2]>,

    /// Accumulated drag delta in world-space (for moving elements).
    pub drag_offset: [f32; 2],

    /// Whether we are drawing a new element (draw shape / draw path / text placement).
    pub is_drawing: bool,

    /// World-space start of the draw operation.
    pub draw_start_world: Option<[f32; 2]>,

    /// World-space current cursor during draw.
    pub draw_current_world: Option<[f32; 2]>,

    /// If editing text inline, the id of the element.
    pub editing_text_id: Option<ElementId>,

    /// Buffer for inline text editing.
    pub text_buffer: String,

    /// Cursor world position (updated every frame).
    pub cursor_world: Option<[f32; 2]>,

    /// Cursor screen position (updated every frame).
    pub cursor_screen: Option<Pos2>,

    /// Whether the canvas has keyboard focus.
    #[allow(dead_code)]
    pub canvas_focused: bool,
}

impl InteractionState {
    pub fn new() -> Self {
        Self {
            mode: ToolMode::Select,
            ..Default::default()
        }
    }

    // ------------------------------------------------------------------
    // Selection helpers
    // ------------------------------------------------------------------

    pub fn select_only(&mut self, id: ElementId) {
        self.selected_ids.clear();
        self.selected_ids.push(id);
    }

    pub fn toggle_selection(&mut self, id: ElementId) {
        if let Some(pos) = self.selected_ids.iter().position(|x| *x == id) {
            self.selected_ids.remove(pos);
        } else {
            self.selected_ids.push(id);
        }
    }

    pub fn clear_selection(&mut self) {
        self.selected_ids.clear();
    }

    #[allow(dead_code)]
    pub fn has_selection(&self) -> bool {
        !self.selected_ids.is_empty()
    }

    /// Return the first selected element id, if any.
    #[allow(dead_code)]
    pub fn primary_selection(&self) -> Option<ElementId> {
        self.selected_ids.first().copied()
    }

    // ------------------------------------------------------------------
    // Drag state management
    // ------------------------------------------------------------------

    pub fn begin_drag(&mut self, world: [f32; 2]) {
        self.is_dragging = true;
        self.drag_start_world = Some(world);
        self.drag_offset = [0.0, 0.0];
    }

    pub fn update_drag(&mut self, world: [f32; 2]) {
        if let Some(start) = self.drag_start_world {
            self.drag_offset = [world[0] - start[0], world[1] - start[1]];
        }
    }

    pub fn end_drag(&mut self) {
        self.is_dragging = false;
        self.drag_start_world = None;
        self.drag_offset = [0.0, 0.0];
    }

    // ------------------------------------------------------------------
    // Draw state management
    // ------------------------------------------------------------------

    pub fn begin_draw(&mut self, world: [f32; 2]) {
        self.is_drawing = true;
        self.draw_start_world = Some(world);
        self.draw_current_world = Some(world);
    }

    pub fn update_draw(&mut self, world: [f32; 2]) {
        self.draw_current_world = Some(world);
    }

    pub fn end_draw(&mut self) -> ([f32; 2], [f32; 2]) {
        self.is_drawing = false;
        let start = self.draw_start_world.take().unwrap_or([0.0, 0.0]);
        let end = self.draw_current_world.take().unwrap_or(start);
        (start, end)
    }

    /// Get the current draw rect (world-space bounding box from start to current).
    pub fn draw_rect(&self) -> Option<[f32; 4]> {
        let s = self.draw_start_world?;
        let c = self.draw_current_world?;
        let l = s[0].min(c[0]);
        let t = s[1].min(c[1]);
        let r = s[0].max(c[0]);
        let b = s[1].max(c[1]);
        Some([l, t, r, b])
    }

    // ------------------------------------------------------------------
    // Text editing
    // ------------------------------------------------------------------

    pub fn begin_text_edit(&mut self, id: ElementId, initial_text: &str) {
        self.editing_text_id = Some(id);
        self.text_buffer = initial_text.to_string();
    }

    pub fn end_text_edit(&mut self) -> (Option<ElementId>, String) {
        let id = self.editing_text_id.take();
        let text = std::mem::take(&mut self.text_buffer);
        (id, text)
    }
}
