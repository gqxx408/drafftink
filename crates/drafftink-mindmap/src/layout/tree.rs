//! 树形布局算法（左右双分支）
//!
//! 用于 MindMap 和 Organization 类型。
//! 根节点居中，左分支向左延伸，右分支向右延伸。

use std::collections::HashMap;
use uuid::Uuid;

use super::{LayoutStrategy, Vec2};
use crate::types::{MindMapDoc, NodePosition};

/// 树形布局策略
pub struct TreeLayout {
    /// 同级节点间距
    pub node_offset: f32,
    /// 子树间距
    pub tree_offset: f32,
    /// 根到一级分支距离
    pub root_distance: f32,
}

impl LayoutStrategy for TreeLayout {
    fn layout(&self, doc: &MindMapDoc, _viewport: Vec2) -> HashMap<Uuid, Vec2> {
        let mut positions = HashMap::new();

        // 根节点在原点（渲染器通过 viewport_offset 负责屏幕居中）
        let root_pos = Vec2::ZERO;
        positions.insert(doc.root_id, root_pos);

        let root = match doc.nodes.get(&doc.root_id) {
            Some(r) => r,
            None => return positions,
        };

        // 分离左右子节点
        let left_children: Vec<Uuid> = root
            .children
            .iter()
            .filter(|&&id| {
                doc.nodes
                    .get(&id)
                    .map(|n| n.position == NodePosition::Left)
                    .unwrap_or(false)
            })
            .copied()
            .collect();

        let right_children: Vec<Uuid> = root
            .children
            .iter()
            .filter(|&&id| {
                doc.nodes
                    .get(&id)
                    .map(|n| n.position == NodePosition::Right)
                    .unwrap_or(false)
            })
            .copied()
            .collect();

        // 计算每个子树的高度
        let subtree_heights = calc_subtree_heights(doc, doc.root_id);

        // 左分支（从根向左延伸）
        if !left_children.is_empty() {
            layout_children(
                doc,
                &left_children,
                &subtree_heights,
                root_pos,
                -1.0,
                self.root_distance,
                self.node_offset,
                self.tree_offset,
                &mut positions,
            );
        }

        // 右分支（从根向右延伸）
        if !right_children.is_empty() {
            layout_children(
                doc,
                &right_children,
                &subtree_heights,
                root_pos,
                1.0,
                self.root_distance,
                self.node_offset,
                self.tree_offset,
                &mut positions,
            );
        }

        positions
    }
}

/// 计算每个节点的子树高度（节点数 × 节点间距）
fn calc_subtree_heights(doc: &MindMapDoc, node_id: Uuid) -> HashMap<Uuid, f32> {
    let mut heights = HashMap::new();
    calc_subtree_height_recursive(doc, node_id, &mut heights);
    heights
}

fn calc_subtree_height_recursive(
    doc: &MindMapDoc,
    node_id: Uuid,
    heights: &mut HashMap<Uuid, f32>,
) -> f32 {
    let node = match doc.nodes.get(&node_id) {
        Some(n) => n,
        None => return 0.0,
    };

    if node.children.is_empty() {
        heights.insert(node_id, 40.0); // 单个节点高度
        return 40.0;
    }

    let mut total = 0.0;
    for &child_id in &node.children {
        total += calc_subtree_height_recursive(doc, child_id, heights);
    }
    heights.insert(node_id, total);
    total
}

/// 递归布局子节点
#[allow(clippy::too_many_arguments)]
fn layout_children(
    doc: &MindMapDoc,
    children: &[Uuid],
    subtree_heights: &HashMap<Uuid, f32>,
    parent_pos: Vec2,
    direction: f32, // 1.0 = 右, -1.0 = 左
    root_distance: f32,
    _node_offset: f32,
    tree_offset: f32,
    positions: &mut HashMap<Uuid, Vec2>,
) {
    // 计算总高度
    let total_height: f32 = children
        .iter()
        .map(|id| subtree_heights.get(id).copied().unwrap_or(40.0))
        .sum::<f32>()
        + (children.len() - 1) as f32 * tree_offset;

    // 起始 Y 偏移（居中）
    let start_y = parent_pos.y - total_height / 2.0;

    let mut current_y = start_y;

    for &child_id in children {
        let child_height = subtree_heights.get(&child_id).copied().unwrap_or(40.0);

        // 子节点位置
        let child_pos = Vec2::new(
            parent_pos.x + direction * root_distance,
            current_y + child_height / 2.0,
        );
        positions.insert(child_id, child_pos);

        // 递归布局孙节点
        if let Some(child) = doc.nodes.get(&child_id) {
            if !child.children.is_empty() && !child.collapsed {
                let grand_children: Vec<Uuid> = child.children.clone();
                layout_children(
                    doc,
                    &grand_children,
                    subtree_heights,
                    child_pos,
                    direction,
                    root_distance,
                    _node_offset,
                    tree_offset,
                    positions,
                );
            }
        }

        current_y += child_height + tree_offset;
    }
}

// ── 测试 ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MindMapDoc;

    #[test]
    fn test_single_node_layout() {
        let doc = MindMapDoc::new("中心主题");
        let layout = TreeLayout {
            node_offset: 12.0,
            tree_offset: 24.0,
            root_distance: 56.0,
        };
        let positions = layout.layout(&doc, Vec2::new(1000.0, 500.0));
        assert_eq!(positions.len(), 1);
        let root_pos = positions[&doc.root_id];
        assert_eq!(root_pos.x, 0.0);
        assert_eq!(root_pos.y, 0.0);
    }

    #[test]
    fn test_two_children_layout() {
        let mut doc = MindMapDoc::new("中心主题");
        let left = doc
            .add_child(doc.root_id, "左节点", NodePosition::Left)
            .unwrap();
        let right = doc
            .add_child(doc.root_id, "右节点", NodePosition::Right)
            .unwrap();

        let layout = TreeLayout {
            node_offset: 12.0,
            tree_offset: 24.0,
            root_distance: 56.0,
        };
        let positions = layout.layout(&doc, Vec2::new(1000.0, 500.0));

        assert_eq!(positions.len(), 3);
        // 左节点在根节点左边
        assert!(positions[&left].x < positions[&doc.root_id].x);
        // 右节点在根节点右边
        assert!(positions[&right].x > positions[&doc.root_id].x);
    }
}
