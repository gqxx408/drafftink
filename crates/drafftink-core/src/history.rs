//! Undo / redo system based on the Command pattern.
//!
//! Every mutation to `CoursewareDoc` goes through a `Command` so that it can
//! be reversed.  The stack is bounded to `MAX_DEPTH` (30) and each command
//! must not exceed `MAX_POINTS` (1000) worth of vertex data.

use std::collections::VecDeque;

use crate::model::{BaseElement, Element, ElementId};

// ---------------------------------------------------------------------------
// Command
// ---------------------------------------------------------------------------

/// A reversible mutation on the courseware document.
#[derive(Debug, Clone)]
pub enum Command {
    /// Push one new element onto the canvas.
    AddElement { element: Element },
    /// Remove an existing element.
    RemoveElement { element: Element, index: usize },
    /// Replace an existing element with a new version.
    ModifyElement {
        id: ElementId,
        old_base: BaseElement,
        new_base: BaseElement,
    },
    /// Move element(s) up or down in the z-order stack.
    ReorderLayer {
        id: ElementId,
        old_z: i32,
        new_z: i32,
    },
}

impl Command {
    /// Approximate vertex count for this command (used to reject oversized
    /// undo entries that would blow memory).
    pub fn point_count(&self) -> usize {
        match self {
            Command::AddElement { element } => element.point_count(),
            Command::RemoveElement { element, .. } => element.point_count(),
            Command::ModifyElement { .. } => 1,
            Command::ReorderLayer { .. } => 1,
        }
    }
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

pub const MAX_DEPTH: usize = 30;
pub const MAX_POINTS: usize = 1000;

#[derive(Debug, Clone)]
pub struct History {
    undo_stack: VecDeque<Command>,
    redo_stack: VecDeque<Command>,
}

impl Default for History {
    fn default() -> Self {
        Self {
            undo_stack: VecDeque::with_capacity(MAX_DEPTH),
            redo_stack: VecDeque::with_capacity(MAX_DEPTH / 2),
        }
    }
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a new command, clearing the redo stack.
    /// Commands exceeding `MAX_POINTS` are silently rejected.
    pub fn push(&mut self, cmd: Command) {
        if cmd.point_count() > MAX_POINTS {
            return;
        }
        self.redo_stack.clear();
        self.undo_stack.push_back(cmd);
        // Pop oldest when over capacity
        while self.undo_stack.len() > MAX_DEPTH {
            self.undo_stack.pop_front();
        }
    }

    /// Pop the most recent undo command, returning None if empty.
    pub fn undo(&mut self) -> Option<Command> {
        let cmd = self.undo_stack.pop_back()?;
        self.redo_stack.push_back(cmd.clone());
        while self.redo_stack.len() > MAX_DEPTH / 2 {
            self.redo_stack.pop_front();
        }
        Some(cmd)
    }

    /// Pop the most recent redo command, returning None if empty.
    pub fn redo(&mut self) -> Option<Command> {
        let cmd = self.redo_stack.pop_back()?;
        self.undo_stack.push_back(cmd.clone());
        while self.undo_stack.len() > MAX_DEPTH {
            self.undo_stack.pop_front();
        }
        Some(cmd)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo_count(&self) -> usize {
        self.undo_stack.len()
    }

    pub fn redo_count(&self) -> usize {
        self.redo_stack.len()
    }
}
