//! 思维导图核心数据模型
//!
//! 所有数据结构都是 `Clone + Serialize + Deserialize`，
//! 方便做 Undo/Redo 快照（直接存储整个 MindMapDoc）。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::layout::Vec2;
use crate::rich_text::RichText;

// ── 导图类型 ──────────────────────────────────────────────────────

/// 导图类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MapType {
    /// 平衡思维导图（左右双分支）
    MindMap,
    /// 鱼骨图
    FishBone,
    /// 组织架构图
    Organization,
    /// 星环图（环形放射状，Mindly 式）
    Mindly,
}

impl Default for MapType {
    fn default() -> Self {
        Self::MindMap
    }
}

// ── 节点位置 ──────────────────────────────────────────────────────

/// 节点在树中的位置（用于左右分支判断）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodePosition {
    /// 根节点
    Root,
    /// 左分支
    Left,
    /// 右分支
    Right,
}

impl Default for NodePosition {
    fn default() -> Self {
        Self::Right
    }
}

// ── 节点样式 ──────────────────────────────────────────────────────

/// 节点视觉样式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStyle {
    /// 节点尺寸
    pub size: Vec2,
    /// 填充颜色 RGBA
    pub fill_color: [u8; 4],
    /// 边框颜色 RGBA
    pub border_color: [u8; 4],
    /// 边框宽度
    pub border_width: f32,
    /// 圆角半径
    pub corner_radius: f32,
    /// 文字颜色 RGBA
    pub text_color: [u8; 4],
    /// 字体大小
    pub font_size: f32,
}

impl Default for NodeStyle {
    fn default() -> Self {
        Self {
            size: Vec2::new(160.0, 40.0),
            fill_color: [60, 65, 80, 255],
            border_color: [80, 85, 100, 255],
            border_width: 1.0,
            corner_radius: 6.0,
            text_color: [220, 225, 240, 255],
            font_size: 14.0,
        }
    }
}

impl NodeStyle {
    /// 根节点专用样式
    pub fn root_style() -> Self {
        Self {
            size: Vec2::new(180.0, 48.0),
            fill_color: [46, 134, 222, 255],
            border_color: [36, 113, 200, 255],
            font_size: 16.0,
            ..Default::default()
        }
    }

    /// 根据层级获取渐变色
    pub fn by_level(level: u32) -> Self {
        let colors: [[u8; 4]; 5] = [
            [46, 134, 222, 255], // 根
            [39, 174, 96, 255],  // L1
            [241, 196, 15, 255], // L2
            [230, 126, 34, 255], // L3
            [155, 89, 182, 255], // L4+
        ];
        let idx = (level as usize).min(colors.len() - 1);
        Self {
            fill_color: colors[idx],
            border_color: [
                colors[idx][0].saturating_sub(20),
                colors[idx][1].saturating_sub(20),
                colors[idx][2].saturating_sub(20),
                255,
            ],
            ..Default::default()
        }
    }
}

// ── 内嵌内容 ──────────────────────────────────────────────────────

/// 节点内嵌元素类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EmbeddedContent {
    /// 图片（路径或 URL）
    Image {
        source: String,
        width: f32,
        height: f32,
    },
    /// 公式（LaTeX）
    Formula(String),
    /// 音频（路径或 URL）
    Audio(String),
    /// 视频（路径或 URL）
    Video(String),
    /// 附件（文件路径）
    Attachment { filename: String, path: String },
}

// ── 子节点旋转配置（星环图专用） ──────────────────────────────────

/// 子节点环的旋转配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildrenRotation {
    /// 起始角度（弧度）
    pub start_angle: f32,
    /// 角度跨度（弧度）
    pub arc_span: f32,
    /// 环半径
    pub radius: f32,
}

impl Default for ChildrenRotation {
    fn default() -> Self {
        Self {
            start_angle: 0.0,
            arc_span: std::f32::consts::TAU,
            radius: 120.0,
        }
    }
}

// ── 思维导图节点 ──────────────────────────────────────────────────

/// 思维导图节点（纯数据，无 UI 依赖）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MindNode {
    /// 节点唯一 ID
    pub id: Uuid,
    /// 父节点 ID（根节点为 None）
    pub parent_id: Option<Uuid>,
    /// 子节点 ID 列表（有序）
    pub children: Vec<Uuid>,
    /// 节点在树中的位置
    pub position: NodePosition,
    /// 层级（根=0）
    pub level: u32,

    // 内容
    /// 富文本标题
    pub title: RichText,
    /// 内嵌元素（公式/图片/音视频）
    pub contents: Vec<EmbeddedContent>,
    /// 节点样式
    pub style: NodeStyle,

    // 状态
    /// 是否收起子节点
    pub collapsed: bool,
    /// 是否隐藏
    pub hidden: bool,

    // 鱼骨图专用
    /// 鱼骨分类标签
    pub fishbone_category: Option<String>,

    // 星环图专用
    /// 子节点环旋转角度
    pub rotation: f32,
    /// 子节点旋转配置
    pub children_rotation: Option<ChildrenRotation>,
}

impl MindNode {
    /// 创建新节点
    pub fn new(id: Uuid, parent_id: Option<Uuid>, title: impl Into<String>, level: u32) -> Self {
        Self {
            id,
            parent_id,
            children: Vec::new(),
            position: NodePosition::default(),
            level,
            title: RichText::plain(title.into()),
            contents: Vec::new(),
            style: if level == 0 {
                NodeStyle::root_style()
            } else {
                NodeStyle::by_level(level)
            },
            collapsed: false,
            hidden: false,
            fishbone_category: None,
            rotation: 0.0,
            children_rotation: None,
        }
    }

    /// 是否是叶子节点
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }

    /// 是否可见（展开路径上所有祖先都没被收起）
    pub fn is_visible(&self, doc: &MindMapDoc) -> bool {
        if self.hidden {
            return false;
        }
        let mut current = self.parent_id;
        while let Some(pid) = current {
            if let Some(parent) = doc.nodes.get(&pid) {
                if parent.collapsed || parent.hidden {
                    return false;
                }
                current = parent.parent_id;
            } else {
                break;
            }
        }
        true
    }
}

// ── 思维导图文档 ──────────────────────────────────────────────────

/// 思维导图文档（完整快照，可直接序列化）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MindMapDoc {
    /// 文档 ID
    pub id: Uuid,
    /// 导图类型
    pub map_type: MapType,
    /// 根节点 ID
    pub root_id: Uuid,
    /// 所有节点（ID → 节点）
    pub nodes: HashMap<Uuid, MindNode>,

    // 星环图模式
    /// 是否 3D 模式
    pub is_3d_mode: bool,
    /// 是否预览模式
    pub is_map_view_mode: bool,
    /// 当前中心节点（星环图的核心）
    pub center_node_id: Uuid,

    // 布局参数
    /// 同级节点间距（希沃默认 12px）
    pub node_offset: f32,
    /// 子树间距（希沃默认 24px）
    pub tree_offset: f32,
    /// 根到一级分支距离（希沃默认 56px）
    pub root_distance: f32,
}

impl MindMapDoc {
    /// 创建空白思维导图
    pub fn new(title: impl Into<String>) -> Self {
        let root_id = Uuid::new_v4();
        let root = MindNode::new(root_id, None, title, 0);
        let mut nodes = HashMap::new();
        nodes.insert(root_id, root);

        Self {
            id: Uuid::new_v4(),
            map_type: MapType::default(),
            root_id,
            nodes,
            is_3d_mode: false,
            is_map_view_mode: false,
            center_node_id: root_id,
            node_offset: 12.0,
            tree_offset: 24.0,
            root_distance: 220.0,
        }
    }

    /// 获取节点的所有子节点 ID（递归）
    pub fn descendants(&self, node_id: Uuid) -> Vec<Uuid> {
        let mut result = Vec::new();
        if let Some(node) = self.nodes.get(&node_id) {
            for &child_id in &node.children {
                result.push(child_id);
                result.extend(self.descendants(child_id));
            }
        }
        result
    }

    /// 获取节点的直接子节点
    pub fn children_of(&self, node_id: Uuid) -> Vec<&MindNode> {
        self.nodes
            .get(&node_id)
            .map(|n| {
                n.children
                    .iter()
                    .filter_map(|id| self.nodes.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 添加子节点
    pub fn add_child(
        &mut self,
        parent_id: Uuid,
        title: impl Into<String>,
        position: NodePosition,
    ) -> anyhow::Result<Uuid> {
        let child_id = Uuid::new_v4();
        let parent = self
            .nodes
            .get_mut(&parent_id)
            .ok_or_else(|| anyhow::anyhow!("父节点不存在: {parent_id}"))?;
        let level = parent.level + 1;
        let mut child = MindNode::new(child_id, Some(parent_id), title, level);
        child.position = position;
        parent.children.push(child_id);
        self.nodes.insert(child_id, child);
        Ok(child_id)
    }

    /// 删除节点及其子节点
    pub fn remove_node(&mut self, node_id: Uuid) -> anyhow::Result<Vec<Uuid>> {
        let mut removed = Vec::new();

        // 递归收集所有后代
        let descendants = self.descendants(node_id);
        removed.push(node_id);
        removed.extend(&descendants);

        // 从父节点移除
        if let Some(node) = self.nodes.get(&node_id) {
            if let Some(parent_id) = node.parent_id {
                if let Some(parent) = self.nodes.get_mut(&parent_id) {
                    parent.children.retain(|&id| id != node_id);
                }
            }
        }

        // 删除所有后代
        for id in &removed {
            self.nodes.remove(id);
        }

        Ok(removed)
    }

    /// 改变节点的父节点（拖拽到别的节点上）
    pub fn change_parent(&mut self, node_id: Uuid, new_parent_id: Uuid) -> anyhow::Result<()> {
        // 防止循环引用
        if self.descendants(node_id).contains(&new_parent_id) {
            return Err(anyhow::anyhow!("不能将节点移动到自己的子节点上"));
        }

        // 从旧父节点移除
        let old_parent_id = self.nodes.get(&node_id).and_then(|n| n.parent_id);
        if let Some(old_pid) = old_parent_id {
            if let Some(old_parent) = self.nodes.get_mut(&old_pid) {
                old_parent.children.retain(|&id| id != node_id);
            }
        }

        // 获取新父节点的层级（在可变借用之前提取）
        let new_level = self
            .nodes
            .get(&new_parent_id)
            .map(|p| p.level + 1)
            .ok_or_else(|| anyhow::anyhow!("目标父节点不存在: {new_parent_id}"))?;

        // 设置新父节点（添加子节点引用）
        if let Some(new_parent) = self.nodes.get_mut(&new_parent_id) {
            new_parent.children.push(node_id);
        }

        // 更新节点自身的父引用和层级
        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.parent_id = Some(new_parent_id);
            node.level = new_level;
        }

        // 更新所有后代的层级
        self.recalc_levels(node_id, new_level)?;

        Ok(())
    }

    /// 切换节点的收起/展开状态
    pub fn toggle_collapse(&mut self, node_id: Uuid) {
        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.collapsed = !node.collapsed;
        }
    }

    /// 切换导图类型
    pub fn switch_type(&mut self, new_type: MapType) {
        self.map_type = new_type;
    }

    /// 重新计算节点层级（递归）
    fn recalc_levels(&mut self, node_id: Uuid, level: u32) -> anyhow::Result<()> {
        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.level = level;
            let child_ids: Vec<Uuid> = node.children.clone();
            for child_id in child_ids {
                self.recalc_levels(child_id, level + 1)?;
            }
        }
        Ok(())
    }
}

// ── 测试 ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_doc() {
        let doc = MindMapDoc::new("中心主题");
        assert_eq!(doc.nodes.len(), 1);
        assert!(doc.nodes.contains_key(&doc.root_id));
    }

    #[test]
    fn test_add_child() {
        let mut doc = MindMapDoc::new("中心主题");
        let child_id = doc
            .add_child(doc.root_id, "子节点", NodePosition::Right)
            .unwrap();
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(doc.nodes[&doc.root_id].children.len(), 1);
        assert_eq!(doc.nodes[&child_id].level, 1);
    }

    #[test]
    fn test_remove_node() {
        let mut doc = MindMapDoc::new("中心主题");
        let child_id = doc
            .add_child(doc.root_id, "子节点", NodePosition::Right)
            .unwrap();
        doc.remove_node(child_id).unwrap();
        assert_eq!(doc.nodes.len(), 1);
        assert!(doc.nodes[&doc.root_id].children.is_empty());
    }

    #[test]
    fn test_change_parent() {
        let mut doc = MindMapDoc::new("中心主题");
        let child_a = doc.add_child(doc.root_id, "A", NodePosition::Left).unwrap();
        let child_b = doc
            .add_child(doc.root_id, "B", NodePosition::Right)
            .unwrap();
        let grandchild = doc.add_child(child_b, "B-1", NodePosition::Right).unwrap();

        // 移动 grandchild 到 child_a 下
        doc.change_parent(grandchild, child_a).unwrap();
        assert_eq!(doc.nodes[&child_a].children, vec![grandchild]);
        assert!(doc.nodes[&child_b].children.is_empty());
        assert_eq!(doc.nodes[&grandchild].level, 2);
    }

    #[test]
    fn test_cannot_move_to_descendant() {
        let mut doc = MindMapDoc::new("中心主题");
        let child = doc
            .add_child(doc.root_id, "子节点", NodePosition::Right)
            .unwrap();
        let result = doc.change_parent(doc.root_id, child);
        assert!(result.is_err());
    }

    #[test]
    fn test_descendants() {
        let mut doc = MindMapDoc::new("中心主题");
        let a = doc
            .add_child(doc.root_id, "A", NodePosition::Right)
            .unwrap();
        let b = doc.add_child(a, "A-1", NodePosition::Right).unwrap();
        let c = doc.add_child(a, "A-2", NodePosition::Right).unwrap();

        let desc = doc.descendants(a);
        assert_eq!(desc.len(), 2);
        assert!(desc.contains(&b));
        assert!(desc.contains(&c));
    }
}
