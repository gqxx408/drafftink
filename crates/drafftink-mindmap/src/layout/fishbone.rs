//! 鱼骨图布局算法
//!
//! 鱼骨图由主骨（水平线）和分支骨（斜线）组成。
//! 主骨从根节点水平延伸，分支骨以 45° 角连接子节点。

use std::collections::HashMap;
use uuid::Uuid;

use super::{LayoutStrategy, Vec2};
use crate::types::MindMapDoc;

/// 鱼骨图布局策略
pub struct FishBoneLayout {
    /// 主骨长度
    pub spine_length: f32,
    /// 分支骨角度（弧度，默认 45°）
    pub branch_angle: f32,
    /// 分支骨长度
    pub branch_length: f32,
    /// 节点间距
    pub node_spacing: f32,
}

impl Default for FishBoneLayout {
    fn default() -> Self {
        Self {
            spine_length: 200.0,
            branch_angle: std::f32::consts::PI / 4.0, // 45°
            branch_length: 80.0,
            node_spacing: 50.0,
        }
    }
}

impl LayoutStrategy for FishBoneLayout {
    fn layout(&self, doc: &MindMapDoc, _viewport: Vec2) -> HashMap<Uuid, Vec2> {
        let mut positions = HashMap::new();

        // 根节点在原点左侧（渲染器通过 viewport_offset 负责屏幕居中）
        let root_pos = Vec2::new(-self.spine_length * 0.5, 0.0);
        positions.insert(doc.root_id, root_pos);

        let root = match doc.nodes.get(&doc.root_id) {
            Some(r) => r,
            None => return positions,
        };

        if root.children.is_empty() {
            return positions;
        }

        // 子节点沿主骨排列
        let n = root.children.len();
        let total_height = (n as f32) * self.node_spacing;
        let start_y = root_pos.y - total_height / 2.0 + self.node_spacing / 2.0;

        for (i, &child_id) in root.children.iter().enumerate() {
            let y = start_y + i as f32 * self.node_spacing;
            let spine_x = root_pos.x + self.spine_length;

            // 子节点在分支骨末端
            let child_pos = Vec2::new(
                spine_x + self.branch_angle.cos() * self.branch_length,
                y + self.branch_angle.sin()
                    * self.branch_length
                    * if i % 2 == 0 { 1.0 } else { -1.0 },
            );
            positions.insert(child_id, child_pos);

            // 递归布局孙节点
            if let Some(child) = doc.nodes.get(&child_id) {
                if !child.children.is_empty() && !child.collapsed {
                    layout_fishbone_grandchildren(
                        doc,
                        child,
                        child_pos,
                        self.branch_length * 0.7,
                        &mut positions,
                    );
                }
            }
        }

        positions
    }
}

/// 递归布局鱼骨图孙节点
fn layout_fishbone_grandchildren(
    doc: &MindMapDoc,
    node: &crate::types::MindNode,
    parent_pos: Vec2,
    branch_len: f32,
    positions: &mut HashMap<Uuid, Vec2>,
) {
    let n = node.children.len();
    if n == 0 {
        return;
    }

    let angle = std::f32::consts::PI / 4.0;
    let spacing = 40.0;

    let total_height = (n as f32 - 1.0) * spacing;
    let start_y = parent_pos.y - total_height / 2.0;

    for (i, &child_id) in node.children.iter().enumerate() {
        let y = start_y + i as f32 * spacing;
        let pos = Vec2::new(parent_pos.x + angle.cos() * branch_len, y);
        positions.insert(child_id, pos);

        if let Some(child) = doc.nodes.get(&child_id) {
            if !child.children.is_empty() && !child.collapsed {
                layout_fishbone_grandchildren(doc, child, pos, branch_len * 0.7, positions);
            }
        }
    }
}

// ── 测试 ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MapType, NodePosition};

    #[test]
    fn test_fishbone_layout() {
        let mut doc = MindMapDoc::new("问题");
        doc.map_type = MapType::FishBone;
        doc.add_child(doc.root_id, "原因1", NodePosition::Right)
            .unwrap();
        doc.add_child(doc.root_id, "原因2", NodePosition::Right)
            .unwrap();
        doc.add_child(doc.root_id, "原因3", NodePosition::Right)
            .unwrap();

        let layout = FishBoneLayout::default();
        let positions = layout.layout(&doc, Vec2::new(1000.0, 500.0));

        assert_eq!(positions.len(), 4);
        // 根节点在左侧
        assert!(positions[&doc.root_id].x < 100.0);
        // 子节点在右侧
        for child_id in &doc.nodes[&doc.root_id].children {
            assert!(positions[child_id].x > positions[&doc.root_id].x);
        }
    }
}
