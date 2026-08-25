//! 星环图布局（极坐标放射状）
//!
//! 用于 Mindly 类型。中心节点位于原点，
//! 子节点以等角度间隔排列在环形轨道上，
//! 孙节点螺旋放射出去。

use std::collections::HashMap;
use uuid::Uuid;

use super::{LayoutStrategy, Vec2};
use crate::types::MindMapDoc;

/// 星环图布局策略
pub struct RadialLayout {
    /// 一级子节点环半径
    pub ring_radius: f32,
    /// 每个子节点最小角度跨度（弧度）
    pub node_angle_spread: f32,
}

impl LayoutStrategy for RadialLayout {
    fn layout(&self, doc: &MindMapDoc, _viewport: Vec2) -> HashMap<Uuid, Vec2> {
        let mut positions = HashMap::new();

        // 中心节点在原点
        let center_id = doc.center_node_id;
        positions.insert(center_id, Vec2::ZERO);

        let center = match doc.nodes.get(&center_id) {
            Some(c) => c,
            None => return positions,
        };

        let children = &center.children;
        let n = children.len() as f32;

        if n == 0.0 {
            return positions;
        }

        // 每个子节点的角度跨度
        let angle_step = std::f32::consts::TAU / n;

        for (i, &child_id) in children.iter().enumerate() {
            let angle = i as f32 * angle_step + center.rotation;
            let radius = self.ring_radius;

            let pos = Vec2::new(angle.cos() * radius, angle.sin() * radius);
            positions.insert(child_id, pos);

            // 递归布局孙节点（螺旋放射）
            if let Some(child) = doc.nodes.get(&child_id) {
                if !child.children.is_empty() && !child.collapsed {
                    let cr = child
                        .children_rotation
                        .as_ref()
                        .map(|cr| cr.radius)
                        .unwrap_or(self.ring_radius * 0.6);
                    let start_angle = child
                        .children_rotation
                        .as_ref()
                        .map(|cr| cr.start_angle)
                        .unwrap_or(angle - 0.3);
                    let arc_span = child
                        .children_rotation
                        .as_ref()
                        .map(|cr| cr.arc_span)
                        .unwrap_or(0.6);

                    layout_grandchildren(
                        doc,
                        &child.children,
                        pos,
                        start_angle,
                        arc_span,
                        cr,
                        &mut positions,
                    );
                }
            }
        }

        positions
    }
}

/// 递归布局孙节点（螺旋放射）
fn layout_grandchildren(
    doc: &MindMapDoc,
    children: &[Uuid],
    parent_pos: Vec2,
    start_angle: f32,
    arc_span: f32,
    radius: f32,
    positions: &mut HashMap<Uuid, Vec2>,
) {
    let n = children.len() as f32;
    if n == 0.0 {
        return;
    }

    let angle_step = if n > 1.0 {
        arc_span / (n - 1.0).max(1.0)
    } else {
        0.0
    };

    for (i, &child_id) in children.iter().enumerate() {
        let angle = start_angle + i as f32 * angle_step;
        let offset = Vec2::new(angle.cos() * radius, angle.sin() * radius);
        let pos = parent_pos + offset;
        positions.insert(child_id, pos);

        // 递归更深层
        if let Some(child) = doc.nodes.get(&child_id) {
            if !child.children.is_empty() && !child.collapsed {
                let cr = child
                    .children_rotation
                    .as_ref()
                    .map(|cr| cr.radius)
                    .unwrap_or(radius * 0.6);
                layout_grandchildren(doc, &child.children, pos, angle - 0.2, 0.4, cr, positions);
            }
        }
    }
}

// ── 测试 ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MapType, NodePosition};

    fn create_radial_doc() -> MindMapDoc {
        let mut doc = MindMapDoc::new("中心主题");
        doc.map_type = MapType::Mindly;
        doc.add_child(doc.root_id, "A", NodePosition::Right)
            .unwrap();
        doc.add_child(doc.root_id, "B", NodePosition::Right)
            .unwrap();
        doc.add_child(doc.root_id, "C", NodePosition::Right)
            .unwrap();
        doc
    }

    #[test]
    fn test_radial_layout() {
        let doc = create_radial_doc();
        let layout = RadialLayout {
            ring_radius: 120.0,
            node_angle_spread: std::f32::consts::TAU / 8.0,
        };
        let positions = layout.layout(&doc, Vec2::new(1000.0, 500.0));

        // 中心节点在原点
        assert_eq!(positions[&doc.root_id], Vec2::ZERO);

        // 3 个子节点
        let children = &doc.nodes[&doc.root_id].children;
        assert_eq!(children.len(), 3);

        // 每个子节点距离中心约 120
        for &child_id in children {
            let pos = positions[&child_id];
            let dist = pos.length();
            assert!((dist - 120.0).abs() < 1.0, "distance: {}", dist);
        }
    }
}
