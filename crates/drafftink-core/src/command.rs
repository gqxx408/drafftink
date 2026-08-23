//! Command pattern — `BoardCommand`, `CommandQueue`, and `UndoRedoStack`.
//!
//! # Data Flow
//!
//! ```text
//!  User Input          UI Layer              BoardContext
//!  ──────────          ────────              ────────────
//!  Click "Add"  →  BoardCommand::AddElement  →  execute_command()
//!  Ctrl+Z       →  —                         →  undo()
//!  Ctrl+Y       →  —                         →  redo()
//!                                            ↓
//!                                     ┌─────────────┐
//!                                     │ CommandQueue │  (pending)
//!                                     └──────┬──────┘
//!                                            │ process()
//!                                            ▼
//!                                     ┌─────────────┐
//!                                     │ UndoRedoStack│  (history)
//!                                     └─────────────┘
//! ```
//!
//! # Design Rules
//!
//! - **Every mutation goes through a command.** No direct state changes.
//! - Commands are **reversible**: each carries enough data to undo itself.
//! - The undo stack is **bounded** to `MAX_HISTORY` (50) entries.
//! - Commands are `Clone` so they can be stored in both undo and redo stacks.

use std::collections::VecDeque;

use crate::element::{ElementData, ElementId};

// ---------------------------------------------------------------------------
// BoardCommand
// ---------------------------------------------------------------------------

/// A reversible mutation on the board state.
///
/// Each variant stores both the "forward" data (needed to apply the command)
/// and the "reverse" data (needed to undo it).  This avoids snapshotting the
/// entire board for every operation.
///
/// # Example
///
/// ```ignore
/// let cmd = BoardCommand::AddElement { element: my_shape };
/// ctx.execute_command(cmd);
/// // ...later...
/// ctx.undo();  // removes my_shape
/// ```
#[derive(Debug, Clone)]
pub enum BoardCommand {
    /// Add a new element to the slide.
    /// *Forward*: push element. *Reverse*: remove element by id.
    AddElement {
        element: ElementData,
    },

    /// Remove an existing element.
    /// *Forward*: remove element by id. *Reverse*: re-insert at original index.
    DeleteElement {
        element: ElementData,
        index: usize,
    },

    /// Move an element by a world-space delta.
    /// *Forward*: add delta to position. *Reverse*: subtract delta.
    MoveElement {
        id: ElementId,
        delta: [f32; 2],
    },

    /// Resize an element.
    /// *Forward*: set new size. *Reverse*: restore old size.
    ResizeElement {
        id: ElementId,
        old_size: [f32; 2],
        new_size: [f32; 2],
    },

    /// Change an element's z-order.
    /// *Forward*: set new z. *Reverse*: restore old z.
    ReorderElement {
        id: ElementId,
        old_z: i32,
        new_z: i32,
    },

    /// Replace an element with a modified version (generic update).
    /// *Forward*: swap to new. *Reverse*: swap back to old.
    ModifyElement {
        old_element: ElementData,
        new_element: ElementData,
    },
}

impl BoardCommand {
    /// Human-readable description for undo/redo menu labels.
    pub fn description(&self) -> &'static str {
        match self {
            BoardCommand::AddElement { .. } => "Add Element",
            BoardCommand::DeleteElement { .. } => "Delete Element",
            BoardCommand::MoveElement { .. } => "Move Element",
            BoardCommand::ResizeElement { .. } => "Resize Element",
            BoardCommand::ReorderElement { .. } => "Reorder Element",
            BoardCommand::ModifyElement { .. } => "Modify Element",
        }
    }
}

// ---------------------------------------------------------------------------
// CommandQueue — pending commands awaiting processing
// ---------------------------------------------------------------------------

/// A FIFO queue of commands that have been submitted but not yet processed
/// by `BoardContext::process_commands()`.
///
/// This decouples command *submission* (from UI or plugins) from command
/// *execution* (in the board update loop).  In practice, the queue is
/// drained every frame.
#[derive(Debug, Clone, Default)]
pub struct CommandQueue {
    pending: VecDeque<BoardCommand>,
}

impl CommandQueue {
    /// Create an empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue a command for later processing.
    pub fn push(&mut self, cmd: BoardCommand) {
        self.pending.push_back(cmd);
    }

    /// Dequeue the next command (FIFO).
    pub fn pop(&mut self) -> Option<BoardCommand> {
        self.pending.pop_front()
    }

    /// Number of pending commands.
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// Whether the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Drain all pending commands, returning them in FIFO order.
    pub fn drain(&mut self) -> Vec<BoardCommand> {
        self.pending.drain(..).collect()
    }
}

// ---------------------------------------------------------------------------
// UndoRedoStack — bounded history of executed commands
// ---------------------------------------------------------------------------

/// Maximum number of undo entries retained.
pub const MAX_HISTORY: usize = 50;

/// A bounded undo/redo stack.
///
/// When `MAX_HISTORY` is exceeded, the oldest undo entry is discarded.
/// Pushing a new command clears the redo stack (standard undo/redo semantics).
#[derive(Debug, Clone)]
pub struct UndoRedoStack {
    undo_stack: VecDeque<BoardCommand>,
    redo_stack: VecDeque<BoardCommand>,
}

impl Default for UndoRedoStack {
    fn default() -> Self {
        Self {
            undo_stack: VecDeque::with_capacity(MAX_HISTORY),
            redo_stack: VecDeque::with_capacity(MAX_HISTORY / 2),
        }
    }
}

impl UndoRedoStack {
    /// Create an empty stack.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an executed command.  Clears the redo stack.
    pub fn push(&mut self, cmd: BoardCommand) {
        self.redo_stack.clear();
        self.undo_stack.push_back(cmd);
        while self.undo_stack.len() > MAX_HISTORY {
            self.undo_stack.pop_front();
        }
    }

    /// Pop the most recent undo entry and move it to the redo stack.
    /// Returns `None` if there is nothing to undo.
    pub fn pop_undo(&mut self) -> Option<BoardCommand> {
        let cmd = self.undo_stack.pop_back()?;
        self.redo_stack.push_back(cmd.clone());
        while self.redo_stack.len() > MAX_HISTORY {
            self.redo_stack.pop_front();
        }
        Some(cmd)
    }

    /// Pop the most recent redo entry and move it back to the undo stack.
    /// Returns `None` if there is nothing to redo.
    pub fn pop_redo(&mut self) -> Option<BoardCommand> {
        let cmd = self.redo_stack.pop_back()?;
        self.undo_stack.push_back(cmd.clone());
        while self.undo_stack.len() > MAX_HISTORY {
            self.undo_stack.pop_front();
        }
        Some(cmd)
    }

    /// Whether undo is available.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Whether redo is available.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Number of undo entries.
    pub fn undo_count(&self) -> usize {
        self.undo_stack.len()
    }

    /// Number of redo entries.
    pub fn redo_count(&self) -> usize {
        self.redo_stack.len()
    }

    /// Clear all history (e.g. when loading a new document).
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// Peek at the most recent undo command without removing it.
    pub fn peek_undo(&self) -> Option<&BoardCommand> {
        self.undo_stack.back()
    }

    /// Peek at the most recent redo command without removing it.
    pub fn peek_redo(&self) -> Option<&BoardCommand> {
        self.redo_stack.back()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::Element;
    use crate::model::{BaseElement, ShapeElement, ShapeType};
    use uuid::Uuid;

    fn make_shape() -> ElementData {
        ElementData::Shape(ShapeElement {
            base: BaseElement {
                id: Uuid::new_v4(),
                position: [0.0, 0.0],
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
    fn command_queue_fifo() {
        let mut q = CommandQueue::new();
        assert!(q.is_empty());

        let e1 = make_shape();
        let e2 = make_shape();
        q.push(BoardCommand::AddElement { element: e1 });
        q.push(BoardCommand::AddElement { element: e2 });

        assert_eq!(q.len(), 2);
        let first = q.pop().unwrap();
        let second = q.pop().unwrap();
        assert!(q.is_empty());
        // FIFO: first pushed should be first popped
        match first {
            BoardCommand::AddElement { element } => assert_eq!(element.position(), [0.0, 0.0]),
            _ => panic!("expected AddElement"),
        }
        let _ = second; // just consume
    }

    #[test]
    fn command_queue_drain() {
        let mut q = CommandQueue::new();
        q.push(BoardCommand::AddElement {
            element: make_shape(),
        });
        q.push(BoardCommand::AddElement {
            element: make_shape(),
        });
        let drained = q.drain();
        assert_eq!(drained.len(), 2);
        assert!(q.is_empty());
    }

    #[test]
    fn undo_redo_basic() {
        let mut stack = UndoRedoStack::new();
        assert!(!stack.can_undo());
        assert!(!stack.can_redo());

        let cmd = BoardCommand::AddElement {
            element: make_shape(),
        };
        stack.push(cmd);

        assert!(stack.can_undo());
        assert!(!stack.can_redo());

        let undone = stack.pop_undo();
        assert!(undone.is_some());
        assert!(!stack.can_undo());
        assert!(stack.can_redo());

        let redone = stack.pop_redo();
        assert!(redone.is_some());
        assert!(stack.can_undo());
        assert!(!stack.can_redo());
    }

    #[test]
    fn push_clears_redo() {
        let mut stack = UndoRedoStack::new();
        stack.push(BoardCommand::AddElement {
            element: make_shape(),
        });
        stack.push(BoardCommand::AddElement {
            element: make_shape(),
        });

        // Undo one
        let _ = stack.pop_undo();
        assert!(stack.can_redo());

        // Pushing a new command should clear redo
        stack.push(BoardCommand::AddElement {
            element: make_shape(),
        });
        assert!(!stack.can_redo());
    }

    #[test]
    fn undo_stack_bounded() {
        let mut stack = UndoRedoStack::new();
        // Push MAX_HISTORY + 20 commands
        for _ in 0..(MAX_HISTORY + 20) {
            stack.push(BoardCommand::AddElement {
                element: make_shape(),
            });
        }
        assert_eq!(stack.undo_count(), MAX_HISTORY);
    }

    #[test]
    fn clear_history() {
        let mut stack = UndoRedoStack::new();
        stack.push(BoardCommand::AddElement {
            element: make_shape(),
        });
        stack.push(BoardCommand::AddElement {
            element: make_shape(),
        });
        stack.clear();
        assert!(!stack.can_undo());
        assert!(!stack.can_redo());
        assert_eq!(stack.undo_count(), 0);
    }

    #[test]
    fn command_description() {
        let cmd = BoardCommand::MoveElement {
            id: Uuid::new_v4(),
            delta: [10.0, 20.0],
        };
        assert_eq!(cmd.description(), "Move Element");
    }

    #[test]
    fn peek_without_removing() {
        let mut stack = UndoRedoStack::new();
        stack.push(BoardCommand::AddElement {
            element: make_shape(),
        });

        let peeked = stack.peek_undo();
        assert!(peeked.is_some());
        assert_eq!(stack.undo_count(), 1); // still there
    }
}
