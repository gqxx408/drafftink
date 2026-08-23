//! `BoardContext` — the central state holder for the whiteboard engine.
//!
//! # Architecture
//!
//! ```text
//!  ┌─────────────────────────────────────────────────────┐
//!  │                   BoardContext                       │
//!  ├─────────────┬───────────────┬──────────┬───────────┤
//!  │   Slide     │ CommandQueue  │ UndoRedo │ BoardMode │
//!  │ (elements)  │ (pending)     │ Stack    │ (Edit/    │
//!  │             │               │          │  Display) │
//!  └─────────────┴───────────────┴──────────┴───────────┘
//!                         │
//!                    Camera (2D)
//! ```
//!
//! # No Global State
//!
//! `BoardContext` is a plain struct — no `static mut`, no `lazy_static`,
//! no `thread_local`.  It is created on the stack (or inside an `App`)
//! and passed by `&mut` to plugins and UI layers.  This eliminates the
//! race conditions and lifetime issues that plagued the original C#
//! codebase's static `QuizStates` and `EditingBoard.Current` singletons.
//!
//! # Dual-Mode Support
//!
//! The context has two modes:
//!
//! - **`EditMode`**: Elements can be selected, dragged, resized, and
//!   deleted.  UI renders edit handles, selection borders, and the
//!   properties panel.
//!
//! - **`DisplayMode`**: Elements are rendered without edit overlays.
//!   Used for presentation / slideshow mode.
//!
//! UI components call `ctx.is_edit_mode()` to decide whether to draw
//! handles, selection rectangles, etc.
//!
//! # Command Flow
//!
//! ```text
//!  UI / Plugin                    BoardContext
//!  ───────────                    ────────────
//!  ctx.enqueue(cmd)  ────────→   CommandQueue
//!                                  │
//!  ctx.process_commands() ←────── │ drain & execute
//!                                  │ push to UndoRedoStack
//!  ctx.undo()         ────────→   pop_undo & reverse
//!  ctx.redo()         ────────→   pop_redo & re-execute
//! ```

use std::collections::HashSet;

use egui::Pos2;

use crate::camera::Camera;
use crate::command::{BoardCommand, CommandQueue, UndoRedoStack};
use crate::element::{Element, ElementData, ElementId};

// ---------------------------------------------------------------------------
// Slide — the current page's element list + page metadata
// ---------------------------------------------------------------------------

/// A single slide (page) in the courseware.
///
/// Contains the element list and page-level metadata.
/// Multi-page documents hold a `Vec<Slide>`.
#[derive(Debug, Clone)]
pub struct Slide {
    /// Elements on this slide, sorted by z_order (lowest first).
    pub elements: Vec<ElementData>,
    /// Page dimensions in world units `[width, height]`.
    pub page_size: [f32; 2],
    /// Background colour `[r, g, b, a]`.
    pub background_color: [u8; 4],
}

impl Default for Slide {
    fn default() -> Self {
        Self {
            elements: Vec::new(),
            page_size: [1920.0, 1080.0],
            background_color: [255, 255, 255, 255],
        }
    }
}

impl Slide {
    /// Create an empty slide with default page size.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a slide with the given page size.
    pub fn with_page_size(page_size: [f32; 2]) -> Self {
        Self {
            page_size,
            ..Default::default()
        }
    }

    /// Find an element by ID.
    pub fn get(&self, id: ElementId) -> Option<&ElementData> {
        self.elements.iter().find(|e| e.id() == id)
    }

    /// Find an element by ID (mutable).
    pub fn get_mut(&mut self, id: ElementId) -> Option<&mut ElementData> {
        self.elements.iter_mut().find(|e| e.id() == id)
    }

    /// Find the index of an element by ID.
    pub fn index_of(&self, id: ElementId) -> Option<usize> {
        self.elements.iter().position(|e| e.id() == id)
    }

    /// Insert an element, assigning it the next available z_order.
    pub fn push(&mut self, element: ElementData) {
        let max_z = self.elements.iter().map(|e| e.z_order()).max().unwrap_or(-1);
        let mut element = element;
        element.set_z_order(max_z + 1);
        self.elements.push(element);
    }

    /// Insert an element at a specific index (used by undo/redo).
    pub fn insert_at(&mut self, index: usize, element: ElementData) {
        if index >= self.elements.len() {
            self.elements.push(element);
        } else {
            self.elements.insert(index, element);
        }
    }

    /// Remove an element by ID, returning it and its index.
    pub fn remove(&mut self, id: ElementId) -> Option<(ElementData, usize)> {
        let index = self.index_of(id)?;
        Some((self.elements.remove(index), index))
    }

    /// Sort elements by z_order (lowest first).
    pub fn sort_by_z(&mut self) {
        self.elements.sort_by_key(|e| e.z_order());
    }

    /// Number of elements on this slide.
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    /// Whether the slide has no elements.
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Hit-test: return the top-most visible, unlocked element at the
    /// given world-space point.
    pub fn hit_test(&self, world_pt: [f32; 2]) -> Option<ElementId> {
        self.elements
            .iter()
            .rev()
            .find(|e| e.visible() && !e.locked() && e.hit_test(world_pt))
            .map(|e| e.id())
    }
}

// ---------------------------------------------------------------------------
// BoardMode — Edit vs Display
// ---------------------------------------------------------------------------

/// The operating mode of the board.
///
/// UI components query `BoardContext::mode()` to decide what to render.
/// For example, edit handles and selection borders are only drawn in
/// `EditMode`.
#[derive(Debug, Clone)]
pub enum BoardMode {
    /// Editing mode — full interaction (select, drag, resize, delete).
    Edit(EditState),

    /// Display / presentation mode — no edit overlays.
    Display(DisplayState),
}

impl Default for BoardMode {
    fn default() -> Self {
        BoardMode::Edit(EditState::default())
    }
}

// ── EditState ──────────────────────────────────────────────────────

/// Mutable state specific to Edit mode.
#[derive(Debug, Clone, Default)]
pub struct EditState {
    /// Currently selected element IDs (supports multi-select).
    pub selected: HashSet<ElementId>,

    /// Marquee-selection start point (screen space).
    pub marquee_start: Option<Pos2>,

    /// Marquee-selection end point (screen space).
    pub marquee_end: Option<Pos2>,

    /// Dash offset for the selection-border animation (incremented per frame).
    pub dash_offset: f32,
}

// ── DisplayState ───────────────────────────────────────────────────

/// Mutable state specific to Display mode.
#[derive(Debug, Clone, Default)]
pub struct DisplayState {
    /// Whether the display is fullscreen (second monitor).
    pub fullscreen: bool,

    /// Current page index for multi-page presentations.
    pub page_index: usize,
}

// ---------------------------------------------------------------------------
// BoardContext
// ---------------------------------------------------------------------------

/// The central state holder for the whiteboard engine.
///
/// Holds the current slide, command queue, undo/redo history, camera,
/// and operating mode.  All mutations go through the command pattern.
///
/// # Example
///
/// ```ignore
/// let mut ctx = BoardContext::new();
///
/// // Add an element
/// ctx.enqueue(BoardCommand::AddElement { element: my_shape });
/// ctx.process_commands();
///
/// // Undo
/// ctx.undo();
///
/// // Switch to display mode
/// ctx.set_display_mode();
/// ```
pub struct BoardContext {
    /// The slide currently being edited or displayed.
    slide: Slide,

    /// Pending commands awaiting execution.
    command_queue: CommandQueue,

    /// Undo / redo history.
    undo_redo: UndoRedoStack,

    /// Current operating mode (Edit or Display).
    mode: BoardMode,

    /// 2D camera for world ↔ screen coordinate transforms.
    camera: Camera,
}

impl Default for BoardContext {
    fn default() -> Self {
        Self::new()
    }
}

impl BoardContext {
    /// Create a new, empty board context in Edit mode.
    pub fn new() -> Self {
        Self {
            slide: Slide::new(),
            command_queue: CommandQueue::new(),
            undo_redo: UndoRedoStack::new(),
            mode: BoardMode::Edit(EditState::default()),
            camera: Camera::default(),
        }
    }

    /// Create a context with a specific page size.
    pub fn with_page_size(page_size: [f32; 2]) -> Self {
        Self {
            slide: Slide::with_page_size(page_size),
            ..Self::new()
        }
    }

    // ── Slide access ───────────────────────────────────────────────

    /// Borrow the current slide.
    pub fn slide(&self) -> &Slide {
        &self.slide
    }

    /// Mutably borrow the current slide.
    pub fn slide_mut(&mut self) -> &mut Slide {
        &mut self.slide
    }

    /// Borrow the element list on the current slide.
    pub fn elements(&self) -> &[ElementData] {
        &self.slide.elements
    }

    /// Mutably borrow the element list on the current slide.
    pub fn elements_mut(&mut self) -> &mut Vec<ElementData> {
        &mut self.slide.elements
    }

    /// Find an element by ID.
    pub fn get_element(&self, id: ElementId) -> Option<&ElementData> {
        self.slide.get(id)
    }

    /// Find an element by ID (mutable).
    pub fn get_element_mut(&mut self, id: ElementId) -> Option<&mut ElementData> {
        self.slide.get_mut(id)
    }

    // ── Mode management ────────────────────────────────────────────

    /// Borrow the current mode.
    pub fn mode(&self) -> &BoardMode {
        &self.mode
    }

    /// Whether the board is in Edit mode.
    pub fn is_edit_mode(&self) -> bool {
        matches!(self.mode, BoardMode::Edit(_))
    }

    /// Whether the board is in Display mode.
    pub fn is_display_mode(&self) -> bool {
        matches!(self.mode, BoardMode::Display(_))
    }

    /// Switch to Edit mode.
    pub fn set_edit_mode(&mut self) {
        self.mode = BoardMode::Edit(EditState::default());
    }

    /// Switch to Display mode.
    pub fn set_display_mode(&mut self) {
        self.mode = BoardMode::Display(DisplayState::default());
    }

    /// Borrow the edit state.  Panics if not in Edit mode.
    pub fn edit_state(&self) -> &EditState {
        match &self.mode {
            BoardMode::Edit(e) => e,
            BoardMode::Display(_) => {
                panic!("edit_state() called while in Display mode")
            }
        }
    }

    /// Mutably borrow the edit state.  Panics if not in Edit mode.
    pub fn edit_state_mut(&mut self) -> &mut EditState {
        match &mut self.mode {
            BoardMode::Edit(e) => e,
            BoardMode::Display(_) => {
                panic!("edit_state_mut() called while in Display mode")
            }
        }
    }

    /// Borrow the display state.  Panics if not in Display mode.
    pub fn display_state(&self) -> &DisplayState {
        match &self.mode {
            BoardMode::Display(d) => d,
            BoardMode::Edit(_) => {
                panic!("display_state() called while in Edit mode")
            }
        }
    }

    /// Mutably borrow the display state.  Panics if not in Display mode.
    pub fn display_state_mut(&mut self) -> &mut DisplayState {
        match &mut self.mode {
            BoardMode::Display(d) => d,
            BoardMode::Edit(_) => {
                panic!("display_state_mut() called while in Edit mode")
            }
        }
    }

    // ── Camera ─────────────────────────────────────────────────────

    /// Borrow the camera.
    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    /// Mutably borrow the camera.
    pub fn camera_mut(&mut self) -> &mut Camera {
        &mut self.camera
    }

    // ── Command queue ──────────────────────────────────────────────

    /// Enqueue a command for later processing.
    ///
    /// The command will be executed when `process_commands()` is called.
    /// This is typically called once per frame from the UI layer.
    pub fn enqueue(&mut self, cmd: BoardCommand) {
        self.command_queue.push(cmd);
    }

    /// Process all pending commands in the queue.
    ///
    /// Each command is executed and pushed onto the undo stack.
    /// Returns the number of commands processed.
    pub fn process_commands(&mut self) -> usize {
        let commands = self.command_queue.drain();
        let count = commands.len();
        for cmd in commands {
            self.execute_command(cmd);
        }
        count
    }

    /// Number of pending commands in the queue.
    pub fn pending_command_count(&self) -> usize {
        self.command_queue.len()
    }

    // ── Undo / Redo ────────────────────────────────────────────────

    /// Whether undo is available.
    pub fn can_undo(&self) -> bool {
        self.undo_redo.can_undo()
    }

    /// Whether redo is available.
    pub fn can_redo(&self) -> bool {
        self.undo_redo.can_redo()
    }

    /// Undo the most recent command.
    pub fn undo(&mut self) -> bool {
        let Some(cmd) = self.undo_redo.pop_undo() else {
            return false;
        };
        self.reverse_command(cmd);
        true
    }

    /// Redo the most recently undone command.
    pub fn redo(&mut self) -> bool {
        let Some(cmd) = self.undo_redo.pop_redo() else {
            return false;
        };
        self.apply_command(cmd);
        true
    }

    /// Clear all undo/redo history (e.g. when loading a new document).
    pub fn clear_history(&mut self) {
        self.undo_redo.clear();
    }

    /// Peek at the description of the next undo command.
    pub fn undo_description(&self) -> Option<&'static str> {
        self.undo_redo.peek_undo().map(|c| c.description())
    }

    /// Peek at the description of the next redo command.
    pub fn redo_description(&self) -> Option<&'static str> {
        self.undo_redo.peek_redo().map(|c| c.description())
    }

    // ── Selection (Edit mode only) ─────────────────────────────────

    /// Select a single element by ID.  Clears previous selection.
    pub fn select(&mut self, id: ElementId) {
        if let BoardMode::Edit(edit) = &mut self.mode {
            edit.selected.clear();
            edit.selected.insert(id);
        }
    }

    /// Toggle selection of an element (Shift+click).
    pub fn toggle_select(&mut self, id: ElementId) {
        if let BoardMode::Edit(edit) = &mut self.mode {
            if edit.selected.contains(&id) {
                edit.selected.remove(&id);
            } else {
                edit.selected.insert(id);
            }
        }
    }

    /// Add an element to the current selection.
    pub fn add_to_selection(&mut self, id: ElementId) {
        if let BoardMode::Edit(edit) = &mut self.mode {
            edit.selected.insert(id);
        }
    }

    /// Clear the current selection.
    pub fn clear_selection(&mut self) {
        if let BoardMode::Edit(edit) = &mut self.mode {
            edit.selected.clear();
        }
    }

    /// Borrow the set of selected element IDs.
    pub fn selected_ids(&self) -> &HashSet<ElementId> {
        match &self.mode {
            BoardMode::Edit(edit) => &edit.selected,
            BoardMode::Display(_) => {
                static EMPTY: std::sync::OnceLock<HashSet<ElementId>> = std::sync::OnceLock::new();
                EMPTY.get_or_init(HashSet::new)
            }
        }
    }

    /// Whether an element is currently selected.
    pub fn is_selected(&self, id: ElementId) -> bool {
        match &self.mode {
            BoardMode::Edit(edit) => edit.selected.contains(&id),
            BoardMode::Display(_) => false,
        }
    }

    /// Number of selected elements.
    pub fn selection_count(&self) -> usize {
        match &self.mode {
            BoardMode::Edit(edit) => edit.selected.len(),
            BoardMode::Display(_) => 0,
        }
    }

    // ── Convenience command constructors ──────────────────────────

    /// Enqueue an "add element" command.
    pub fn add_element(&mut self, element: ElementData) {
        self.enqueue(BoardCommand::AddElement { element });
    }

    /// Enqueue a "delete element" command for the given ID.
    /// If the element doesn't exist, this is a no-op.
    pub fn delete_element(&mut self, id: ElementId) {
        if let Some((element, index)) = self.slide.remove(id) {
            self.enqueue(BoardCommand::DeleteElement { element, index });
        }
    }

    /// Enqueue a "move element" command.
    pub fn move_element(&mut self, id: ElementId, delta: [f32; 2]) {
        self.enqueue(BoardCommand::MoveElement { id, delta });
    }

    /// Enqueue a "resize element" command.
    pub fn resize_element(&mut self, id: ElementId, new_size: [f32; 2]) {
        if let Some(element) = self.slide.get(id) {
            let old_size = element.size();
            self.enqueue(BoardCommand::ResizeElement {
                id,
                old_size,
                new_size,
            });
        }
    }

    // ── Internal: command execution ────────────────────────────────

    /// Execute a command and push it onto the undo stack.
    fn execute_command(&mut self, cmd: BoardCommand) {
        self.apply_command(cmd.clone());
        self.undo_redo.push(cmd);
    }

    /// Apply a command's forward effect to the slide.
    fn apply_command(&mut self, cmd: BoardCommand) {
        match cmd {
            BoardCommand::AddElement { element } => {
                self.slide.push(element);
            }
            BoardCommand::DeleteElement { element, index } => {
                // Remove by ID (in case the list has changed since enqueue)
                let id = element.id();
                self.slide.remove(id);
                let _ = index; // index used only for undo
            }
            BoardCommand::MoveElement { id, delta } => {
                if let Some(e) = self.slide.get_mut(id) {
                    let pos = e.position();
                    e.set_position([pos[0] + delta[0], pos[1] + delta[1]]);
                }
            }
            BoardCommand::ResizeElement {
                id, new_size, ..
            } => {
                if let Some(e) = self.slide.get_mut(id) {
                    e.set_size(new_size);
                }
            }
            BoardCommand::ReorderElement { id, new_z, .. } => {
                if let Some(e) = self.slide.get_mut(id) {
                    e.set_z_order(new_z);
                }
                self.slide.sort_by_z();
            }
            BoardCommand::ModifyElement { new_element, .. } => {
                let id = new_element.id();
                if let Some(idx) = self.slide.index_of(id) {
                    self.slide.elements[idx] = new_element;
                }
            }
        }
    }

    /// Reverse a command's effect (for undo).
    fn reverse_command(&mut self, cmd: BoardCommand) {
        match cmd {
            BoardCommand::AddElement { element } => {
                self.slide.remove(element.id());
            }
            BoardCommand::DeleteElement { element, index } => {
                self.slide.insert_at(index, element);
            }
            BoardCommand::MoveElement { id, delta } => {
                if let Some(e) = self.slide.get_mut(id) {
                    let pos = e.position();
                    e.set_position([pos[0] - delta[0], pos[1] - delta[1]]);
                }
            }
            BoardCommand::ResizeElement {
                id, old_size, ..
            } => {
                if let Some(e) = self.slide.get_mut(id) {
                    e.set_size(old_size);
                }
            }
            BoardCommand::ReorderElement { id, old_z, .. } => {
                if let Some(e) = self.slide.get_mut(id) {
                    e.set_z_order(old_z);
                }
                self.slide.sort_by_z();
            }
            BoardCommand::ModifyElement { old_element, .. } => {
                let id = old_element.id();
                if let Some(idx) = self.slide.index_of(id) {
                    self.slide.elements[idx] = old_element;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BaseElement, ShapeElement, ShapeType};
    use uuid::Uuid;

    fn make_shape(pos: [f32; 2]) -> ElementData {
        ElementData::Shape(ShapeElement {
            base: BaseElement {
                id: Uuid::new_v4(),
                position: pos,
                size: [100.0, 100.0],
                ..Default::default()
            },
            shape_type: ShapeType::Rectangle,
            has_start_arrow: false,
            has_end_arrow: false,
            scale_y: 0.0,
        })
    }

    #[test]
    fn slide_push_and_get() {
        let mut slide = Slide::new();
        let shape = make_shape([10.0, 20.0]);
        let id = shape.id();

        slide.push(shape);
        assert_eq!(slide.len(), 1);

        let found = slide.get(id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().position(), [10.0, 20.0]);
    }

    #[test]
    fn slide_push_assigns_z_order() {
        let mut slide = Slide::new();
        slide.push(make_shape([0.0, 0.0]));
        slide.push(make_shape([0.0, 0.0]));

        assert_eq!(slide.elements[0].z_order(), 0);
        assert_eq!(slide.elements[1].z_order(), 1);
    }

    #[test]
    fn slide_remove_returns_element_and_index() {
        let mut slide = Slide::new();
        slide.push(make_shape([0.0, 0.0]));
        slide.push(make_shape([10.0, 10.0]));
        let id = slide.elements[0].id();

        let removed = slide.remove(id);
        assert!(removed.is_some());
        let (elem, idx) = removed.unwrap();
        assert_eq!(idx, 0);
        assert_eq!(elem.position(), [0.0, 0.0]);
        assert_eq!(slide.len(), 1);
    }

    #[test]
    fn slide_hit_test_topmost() {
        let mut slide = Slide::new();
        let bottom = make_shape([0.0, 0.0]);
        let top = make_shape([0.0, 0.0]);
        let top_id = top.id();

        slide.push(bottom);
        slide.push(top); // higher z_order → on top

        let hit = slide.hit_test([50.0, 50.0]);
        assert_eq!(hit, Some(top_id));
    }

    #[test]
    fn slide_hit_test_skips_invisible() {
        let mut slide = Slide::new();
        let mut invisible = make_shape([0.0, 0.0]);
        invisible.set_size([100.0, 100.0]);
        // Make invisible by using base field
        if let ElementData::Shape(ref mut s) = invisible {
            s.base.visible = false;
        }
        slide.push(invisible);

        let hit = slide.hit_test([50.0, 50.0]);
        assert_eq!(hit, None);
    }

    #[test]
    fn board_context_add_and_undo() {
        let mut ctx = BoardContext::new();
        let shape = make_shape([0.0, 0.0]);
        let id = shape.id();

        ctx.add_element(shape);
        assert_eq!(ctx.pending_command_count(), 1);

        ctx.process_commands();
        assert_eq!(ctx.elements().len(), 1);
        assert!(ctx.can_undo());

        ctx.undo();
        assert_eq!(ctx.elements().len(), 0);
        assert!(!ctx.can_undo());
        assert!(ctx.can_redo());

        ctx.redo();
        assert_eq!(ctx.elements().len(), 1);
        assert_eq!(ctx.elements()[0].id(), id);
    }

    #[test]
    fn board_context_move_and_undo() {
        let mut ctx = BoardContext::new();
        let shape = make_shape([100.0, 100.0]);
        let id = shape.id();

        ctx.add_element(shape);
        ctx.process_commands();

        ctx.move_element(id, [50.0, -30.0]);
        ctx.process_commands();
        assert_eq!(ctx.get_element(id).unwrap().position(), [150.0, 70.0]);

        ctx.undo();
        assert_eq!(ctx.get_element(id).unwrap().position(), [100.0, 100.0]);

        ctx.redo();
        assert_eq!(ctx.get_element(id).unwrap().position(), [150.0, 70.0]);
    }

    #[test]
    fn board_context_delete_and_undo() {
        let mut ctx = BoardContext::new();
        let shape = make_shape([0.0, 0.0]);
        let id = shape.id();

        ctx.add_element(shape);
        ctx.process_commands();
        assert_eq!(ctx.elements().len(), 1);

        ctx.delete_element(id);
        ctx.process_commands();
        assert_eq!(ctx.elements().len(), 0);

        ctx.undo();
        assert_eq!(ctx.elements().len(), 1);
        assert_eq!(ctx.elements()[0].id(), id);
    }

    #[test]
    fn board_context_resize_and_undo() {
        let mut ctx = BoardContext::new();
        let shape = make_shape([0.0, 0.0]);
        let id = shape.id();

        ctx.add_element(shape);
        ctx.process_commands();

        ctx.resize_element(id, [200.0, 150.0]);
        ctx.process_commands();
        assert_eq!(ctx.get_element(id).unwrap().size(), [200.0, 150.0]);

        ctx.undo();
        assert_eq!(ctx.get_element(id).unwrap().size(), [100.0, 100.0]);
    }

    #[test]
    fn board_context_mode_switch() {
        let mut ctx = BoardContext::new();
        assert!(ctx.is_edit_mode());

        ctx.set_display_mode();
        assert!(ctx.is_display_mode());
        assert!(!ctx.is_edit_mode());
        assert_eq!(ctx.selection_count(), 0); // no selection in display mode

        ctx.set_edit_mode();
        assert!(ctx.is_edit_mode());
    }

    #[test]
    fn board_context_selection() {
        let mut ctx = BoardContext::new();
        let shape = make_shape([0.0, 0.0]);
        let id = shape.id();

        ctx.add_element(shape);
        ctx.process_commands();

        ctx.select(id);
        assert!(ctx.is_selected(id));
        assert_eq!(ctx.selection_count(), 1);

        ctx.toggle_select(id);
        assert!(!ctx.is_selected(id));
        assert_eq!(ctx.selection_count(), 0);

        ctx.add_to_selection(id);
        assert!(ctx.is_selected(id));

        ctx.clear_selection();
        assert_eq!(ctx.selection_count(), 0);
    }

    #[test]
    fn board_context_multiple_undo_redo() {
        let mut ctx = BoardContext::new();

        // Add three elements
        for i in 0..3 {
            ctx.add_element(make_shape([i as f32 * 100.0, 0.0]));
        }
        ctx.process_commands();
        assert_eq!(ctx.elements().len(), 3);

        // Undo all three
        ctx.undo();
        assert_eq!(ctx.elements().len(), 2);
        ctx.undo();
        assert_eq!(ctx.elements().len(), 1);
        ctx.undo();
        assert_eq!(ctx.elements().len(), 0);
        assert!(!ctx.can_undo());

        // Redo all three
        ctx.redo();
        assert_eq!(ctx.elements().len(), 1);
        ctx.redo();
        assert_eq!(ctx.elements().len(), 2);
        ctx.redo();
        assert_eq!(ctx.elements().len(), 3);
        assert!(!ctx.can_redo());
    }

    #[test]
    fn board_context_clear_history() {
        let mut ctx = BoardContext::new();
        ctx.add_element(make_shape([0.0, 0.0]));
        ctx.process_commands();
        assert!(ctx.can_undo());

        ctx.clear_history();
        assert!(!ctx.can_undo());
        assert!(!ctx.can_redo());
    }

    #[test]
    fn board_context_undo_description() {
        let mut ctx = BoardContext::new();
        ctx.add_element(make_shape([0.0, 0.0]));
        ctx.process_commands();

        assert_eq!(ctx.undo_description(), Some("Add Element"));
    }

    #[test]
    fn board_context_no_global_state() {
        // This test verifies that two BoardContext instances are fully independent.
        let mut ctx_a = BoardContext::new();
        let ctx_b = BoardContext::new();

        ctx_a.add_element(make_shape([0.0, 0.0]));
        ctx_a.process_commands();

        // ctx_b should be unaffected
        assert_eq!(ctx_a.elements().len(), 1);
        assert_eq!(ctx_b.elements().len(), 0);
    }
}
