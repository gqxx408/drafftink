//! 交互控制器
//!
//! 对应希沃的 4 个 Controller（NodeController / DragController /
//! SizeController / StateController），用 Rust 状态机 + 事件模式实现。
//!
//! # 设计
//! - 所有交互状态存储在 `MindMapInteraction` 中
//! - 事件通过 `handle_event` 分发到对应的处理逻辑
//! - 处理结果直接修改 `MindMapDoc`（通过事件驱动）

use egui::Pos2;
use uuid::Uuid;

use crate::layout::Vec2;
use crate::types::{MapType, MindMapDoc, NodePosition};

/// 交互状态机
#[derive(Debug, Clone, Default)]
pub struct MindMapInteraction {
    /// 当前悬停的节点 ID
    pub hovered_node: Option<Uuid>,
    /// 当前选中的节点 ID
    pub selected_node: Option<Uuid>,
    /// 拖拽状态
    pub dragging: Option<DragState>,
    /// 正在编辑文本的节点 ID
    pub editing_text: Option<Uuid>,
    /// 文本编辑缓冲区
    pub text_buffer: String,
    /// 视口拖拽状态
    pub panning: Option<PanState>,
    /// 是否正在添加子节点模式
    pub adding_child: bool,
}

/// 拖拽状态
#[derive(Debug, Clone)]
pub struct DragState {
    /// 被拖拽的节点 ID
    pub node_id: Uuid,
    /// 拖拽起始位置（屏幕坐标）
    pub start_pos: Pos2,
    /// 当前偏移量
    pub offset: Vec2,
}

/// 视口平移状态
#[derive(Debug, Clone)]
pub struct PanState {
    /// 平移起始位置
    pub start_pos: Pos2,
}

/// 导图交互事件
#[derive(Debug, Clone)]
pub enum MindMapEvent {
    /// 选中节点
    SelectNode(Uuid),
    /// 取消选中
    Deselect,
    /// 切换展开/收起
    ToggleCollapse(Uuid),
    /// 开始拖拽节点
    StartDrag(Uuid, Pos2),
    /// 拖拽中
    DragTo(Pos2),
    /// 拖拽结束（释放鼠标）
    DropOn(Uuid),
    /// 拖拽取消
    DragCancel,
    /// 添加子节点
    AddChild(Uuid, NodePosition),
    /// 添加兄弟节点
    AddSibling(Uuid, NodePosition),
    /// 删除节点
    DeleteNode(Uuid),
    /// 开始编辑文本
    StartEditText(Uuid),
    /// 编辑文本
    EditText(Uuid, String),
    /// 结束编辑文本
    FinishEditText,
    /// 切换导图类型
    SwitchMapType(MapType),
    /// 切换 3D 模式
    Toggle3DMode,
    /// 开始视口平移
    StartPan(Pos2),
    /// 视口平移中
    PanTo(Pos2),
    /// 结束视口平移
    EndPan,
    /// 缩放
    Zoom(f32),
    /// 重置缩放
    ResetZoom,
    /// 悬停节点
    Hover(Uuid),
    /// 离开节点
    Unhover,
    /// 键盘快捷键
    KeyPress(KeyAction),
}

/// 键盘操作
#[derive(Debug, Clone, Copy)]
pub enum KeyAction {
    /// Tab - 添加子节点
    Tab,
    /// Enter - 添加兄弟节点
    Enter,
    /// Delete / Backspace - 删除选中节点
    Delete,
    /// Escape - 取消选择/编辑
    Escape,
    /// F2 - 重命名
    Rename,
}

impl MindMapInteraction {
    /// 处理交互事件
    ///
    /// 返回 `true` 表示文档已修改，需要重新布局和重绘。
    pub fn handle_event(
        &mut self,
        event: MindMapEvent,
        doc: &mut MindMapDoc,
    ) -> anyhow::Result<bool> {
        match event {
            // ── 选择 ──────────────────────────────────────────
            MindMapEvent::SelectNode(id) => {
                self.selected_node = Some(id);
                self.adding_child = false;
                Ok(false) // 只改变 UI 状态，不修改文档
            }
            MindMapEvent::Deselect => {
                self.selected_node = None;
                self.editing_text = None;
                self.adding_child = false;
                Ok(false)
            }

            // ── 展开/收起 ─────────────────────────────────────
            MindMapEvent::ToggleCollapse(id) => {
                doc.toggle_collapse(id);
                Ok(true) // 文档结构变化，需要重新布局
            }

            // ── 拖拽 ──────────────────────────────────────────
            MindMapEvent::StartDrag(id, pos) => {
                self.dragging = Some(DragState {
                    node_id: id,
                    start_pos: pos,
                    offset: Vec2::ZERO,
                });
                Ok(false)
            }
            MindMapEvent::DragTo(pos) => {
                if let Some(ref mut drag) = self.dragging {
                    drag.offset = Vec2::new(pos.x - drag.start_pos.x, pos.y - drag.start_pos.y);
                }
                Ok(false) // 拖拽中不修改文档，只更新视觉偏移
            }
            MindMapEvent::DropOn(target_id) => {
                if let Some(ref drag) = self.dragging {
                    if drag.node_id != target_id {
                        // 改变父子关系
                        doc.change_parent(drag.node_id, target_id)?;
                        self.dragging = None;
                        return Ok(true); // 文档结构变化
                    }
                }
                self.dragging = None;
                Ok(false)
            }
            MindMapEvent::DragCancel => {
                self.dragging = None;
                Ok(false)
            }

            // ── 添加节点 ──────────────────────────────────────
            MindMapEvent::AddChild(parent_id, position) => {
                let child_id = doc.add_child(parent_id, "新节点", position)?;
                self.selected_node = Some(child_id);
                self.start_edit(child_id);
                Ok(true)
            }
            MindMapEvent::AddSibling(node_id, position) => {
                let parent_id = doc
                    .nodes
                    .get(&node_id)
                    .and_then(|n| n.parent_id)
                    .unwrap_or(doc.root_id);
                let sibling_id = doc.add_child(parent_id, "新节点", position)?;
                self.selected_node = Some(sibling_id);
                self.start_edit(sibling_id);
                Ok(true)
            }

            // ── 删除节点 ──────────────────────────────────────
            MindMapEvent::DeleteNode(id) => {
                if id == doc.root_id {
                    return Err(anyhow::anyhow!("不能删除根节点"));
                }
                doc.remove_node(id)?;
                self.selected_node = None;
                Ok(true)
            }

            // ── 文本编辑 ──────────────────────────────────────
            MindMapEvent::StartEditText(id) => {
                self.start_edit(id);
                Ok(false)
            }
            MindMapEvent::EditText(id, text) => {
                if let Some(node) = doc.nodes.get_mut(&id) {
                    node.title = crate::rich_text::RichText::plain(text);
                }
                Ok(true) // 内容变化，需要重绘
            }
            MindMapEvent::FinishEditText => {
                self.editing_text = None;
                self.text_buffer.clear();
                Ok(false)
            }

            // ── 类型切换 ──────────────────────────────────────
            MindMapEvent::SwitchMapType(new_type) => {
                doc.switch_type(new_type);
                Ok(true) // 需要重新布局
            }

            // ── 3D 模式 ───────────────────────────────────────
            MindMapEvent::Toggle3DMode => {
                doc.is_3d_mode = !doc.is_3d_mode;
                Ok(true)
            }

            // ── 视口操作 ──────────────────────────────────────
            MindMapEvent::StartPan(pos) => {
                self.panning = Some(PanState { start_pos: pos });
                Ok(false)
            }
            MindMapEvent::PanTo(_pos) => {
                // 视口平移由外部处理（renderer.viewport_offset）
                Ok(false)
            }
            MindMapEvent::EndPan => {
                self.panning = None;
                Ok(false)
            }
            MindMapEvent::Zoom(_delta) => {
                // 缩放由外部处理
                Ok(false)
            }
            MindMapEvent::ResetZoom => Ok(false),

            // ── 悬停 ──────────────────────────────────────────
            MindMapEvent::Hover(id) => {
                self.hovered_node = Some(id);
                Ok(false)
            }
            MindMapEvent::Unhover => {
                self.hovered_node = None;
                Ok(false)
            }

            // ── 键盘快捷键 ────────────────────────────────────
            MindMapEvent::KeyPress(action) => self.handle_key(action, doc),
        }
    }

    /// 处理键盘快捷键
    fn handle_key(&mut self, action: KeyAction, doc: &mut MindMapDoc) -> anyhow::Result<bool> {
        match action {
            KeyAction::Tab => {
                if let Some(selected) = self.selected_node {
                    let position = doc
                        .nodes
                        .get(&selected)
                        .map(|n| n.position)
                        .unwrap_or(NodePosition::Right);
                    let child_id = doc.add_child(selected, "新节点", position)?;
                    self.selected_node = Some(child_id);
                    self.start_edit(child_id);
                    return Ok(true);
                }
                Ok(false)
            }
            KeyAction::Enter => {
                if let Some(selected) = self.selected_node {
                    if selected == doc.root_id {
                        // 根节点下添加
                        let child_id = doc.add_child(selected, "新节点", NodePosition::Right)?;
                        self.selected_node = Some(child_id);
                        self.start_edit(child_id);
                    } else {
                        let parent_id = doc
                            .nodes
                            .get(&selected)
                            .and_then(|n| n.parent_id)
                            .unwrap_or(doc.root_id);
                        let position = doc
                            .nodes
                            .get(&selected)
                            .map(|n| n.position)
                            .unwrap_or(NodePosition::Right);
                        let sibling_id = doc.add_child(parent_id, "新节点", position)?;
                        self.selected_node = Some(sibling_id);
                        self.start_edit(sibling_id);
                    }
                    return Ok(true);
                }
                Ok(false)
            }
            KeyAction::Delete => {
                if let Some(selected) = self.selected_node {
                    if selected != doc.root_id {
                        doc.remove_node(selected)?;
                        self.selected_node = None;
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            KeyAction::Escape => {
                self.selected_node = None;
                self.editing_text = None;
                self.adding_child = false;
                Ok(false)
            }
            KeyAction::Rename => {
                if let Some(selected) = self.selected_node {
                    self.start_edit(selected);
                }
                Ok(false)
            }
        }
    }

    /// 开始编辑节点文本
    fn start_edit(&mut self, node_id: Uuid) {
        self.editing_text = Some(node_id);
        self.text_buffer.clear();
    }
}

// ── 测试 ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_deselect() {
        let mut doc = MindMapDoc::new("中心主题");
        let mut interaction = MindMapInteraction::default();

        interaction
            .handle_event(MindMapEvent::SelectNode(doc.root_id), &mut doc)
            .unwrap();
        assert_eq!(interaction.selected_node, Some(doc.root_id));

        interaction
            .handle_event(MindMapEvent::Deselect, &mut doc)
            .unwrap();
        assert_eq!(interaction.selected_node, None);
    }

    #[test]
    fn test_add_child_via_event() {
        let mut doc = MindMapDoc::new("中心主题");
        let mut interaction = MindMapInteraction::default();

        let changed = interaction
            .handle_event(
                MindMapEvent::AddChild(doc.root_id, NodePosition::Right),
                &mut doc,
            )
            .unwrap();

        assert!(changed);
        assert_eq!(doc.nodes.len(), 2);
        assert!(interaction.selected_node.is_some());
        assert!(interaction.editing_text.is_some());
    }

    #[test]
    fn test_delete_node() {
        let mut doc = MindMapDoc::new("中心主题");
        let child_id = doc
            .add_child(doc.root_id, "子节点", NodePosition::Right)
            .unwrap();
        let mut interaction = MindMapInteraction::default();

        let changed = interaction
            .handle_event(MindMapEvent::DeleteNode(child_id), &mut doc)
            .unwrap();

        assert!(changed);
        assert_eq!(doc.nodes.len(), 1);
    }

    #[test]
    fn test_cannot_delete_root() {
        let mut doc = MindMapDoc::new("中心主题");
        let mut interaction = MindMapInteraction::default();

        let result = interaction.handle_event(MindMapEvent::DeleteNode(doc.root_id), &mut doc);
        assert!(result.is_err());
    }

    #[test]
    fn test_toggle_collapse() {
        let mut doc = MindMapDoc::new("中心主题");
        doc.add_child(doc.root_id, "子节点", NodePosition::Right)
            .unwrap();
        let mut interaction = MindMapInteraction::default();

        interaction
            .handle_event(MindMapEvent::ToggleCollapse(doc.root_id), &mut doc)
            .unwrap();

        assert!(doc.nodes[&doc.root_id].collapsed);

        interaction
            .handle_event(MindMapEvent::ToggleCollapse(doc.root_id), &mut doc)
            .unwrap();

        assert!(!doc.nodes[&doc.root_id].collapsed);
    }
}
