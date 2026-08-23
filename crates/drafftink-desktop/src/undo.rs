//! 桌面端 undo / redo 命令栈（命令模式）。
//!
//! 覆盖两条数据层：
//! 1. **文档层**（`CoursewareDoc.pages[*].elements`，legacy `Element`）：文本的插入 /
//!    删除 / 内容修改；
//! 2. **宿主叠加层**（形状 / 图片 / 视频 / 音频的 `HashMap` 实例）：插入 / 删除。
//!    视频 / 音频实例不可 `Clone`（持有子进程），故用其「记录」在撤销时按路径重建播放器。
//!
//! 栈上限 100 条，超出丢弃最老（内存有界）。

use std::collections::VecDeque;

use egui::Rect;
use uuid::Uuid;

use drafftink_core::model::Element;

use crate::app::{
    FunctionPlotInstance, ImageInstance, InsertedAudio, InsertedVideo, SelectedElement,
    ShapeInstance,
};

/// 一次可逆的宿主层 / 文档层变更。
#[derive(Clone)]
pub(crate) enum UndoCmd {
    // ── 文档层：文本（Element::Text） ────────────────────────────────
    /// 插入了一个文档元素（撤销 = 删除；重做 = 重新插入）。
    InsertElement { page: usize, elem: Element },
    /// 删除了一个文档元素（撤销 = 按索引插回；重做 = 再次删除）。
    RemoveElement { page: usize, index: usize, elem: Element },
    /// 修改了文档层文本（移动 / 缩放 / 内容编辑；撤销 = 回旧值，重做 = 应用新值）。
    ModifyText { page: usize, elem_id: Uuid, old: Element, new: Element },

    // ── 宿主叠加层：形状 ────────────────────────────────────────────
    InsertShape { id: String, inst: ShapeInstance },
    RemoveShape { id: String, inst: ShapeInstance },

    // ── 宿主叠加层：图片 ────────────────────────────────────────────
    InsertImage { id: String, inst: ImageInstance },
    RemoveImage { id: String, inst: ImageInstance },

    // ── 宿主叠加层：视频（按记录重建播放器） ────────────────────────
    InsertVideo { id: String, record: InsertedVideo },
    RemoveVideo { id: String, record: InsertedVideo },

    // ── 宿主叠加层：音频（按记录重建播放器） ────────────────────────
    InsertAudio { id: String, record: InsertedAudio },
    RemoveAudio { id: String, record: InsertedAudio },

    // ── 宿主叠加层：移动 / 缩放（user_rect 变化，撤销 = 回旧矩形） ──
    ModifyRect {
        sel: SelectedElement,
        old_rect: Option<Rect>,
        new_rect: Option<Rect>,
    },

    // ── 宿主叠加层：函数绘图（坐标系 + 曲线实例） ────────────────────
    InsertFunction { id: String, inst: FunctionPlotInstance },
    RemoveFunction { id: String, inst: FunctionPlotInstance },
}

/// 撤销栈上限。
const MAX_DEPTH: usize = 100;

/// 撤销 / 重做历史（命令模式的栈）。
#[derive(Default)]
pub(crate) struct UndoHistory {
    undo: VecDeque<UndoCmd>,
    redo: VecDeque<UndoCmd>,
}

impl UndoHistory {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// 记录一次新操作，清空重做栈；超出上限丢弃最老。
    pub(crate) fn push(&mut self, cmd: UndoCmd) {
        self.redo.clear();
        self.undo.push_back(cmd);
        while self.undo.len() > MAX_DEPTH {
            self.undo.pop_front();
        }
    }

    /// 弹出最近一次操作（调用方应用其**逆**操作），None 表示无操作可撤销。
    pub(crate) fn undo(&mut self) -> Option<UndoCmd> {
        let cmd = self.undo.pop_back()?;
        self.redo.push_back(cmd.clone());
        while self.redo.len() > MAX_DEPTH {
            self.redo.pop_front();
        }
        Some(cmd)
    }

    /// 弹出最近一次被撤销的操作（调用方**重新应用**它），None 表示无操作可重做。
    pub(crate) fn redo(&mut self) -> Option<UndoCmd> {
        let cmd = self.redo.pop_back()?;
        self.undo.push_back(cmd.clone());
        while self.undo.len() > MAX_DEPTH {
            self.undo.pop_front();
        }
        Some(cmd)
    }
}
