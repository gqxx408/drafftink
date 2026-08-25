//! 板书笔迹的四叉树空间索引。
//!
//! ## 为什么需要
//!
//! 板书批注的笔迹全部存于屏幕坐标系（`InkStroke.points` 直接来自指针位置），
//! 数量随课堂推进单调增长。没有空间索引时，两个高频操作都是 O(全部点数)：
//!
//! - **渲染**：每帧遍历所有笔迹，哪怕它们完全在视口之外。
//! - **橡皮**：每次拖动都要扫描所有笔迹的所有点做距离测试。
//!
//! 引入四叉树后，两者都退化为 O(命中区域内的笔迹数)：老师在左上角擦除时，
//! 右下角象限的笔迹连碰都不会碰。
//!
//! ## 设计取舍
//!
//! - 索引里只存**笔迹下标 + 包围盒**，不复制点数据，索引本身极轻量。
//! - 跨越子节点边界的笔迹**留在父节点**，避免重复存储与去重开销。
//! - 深度与单节点容量都有上限，退化输入（如整屏一条长线）不会导致无限分裂。
//! - 笔迹集合变更后由调用方置脏并整体 `rebuild`，重建是 O(n) 且只在变更帧发生。

use egui::{Pos2, Rect, Vec2};

use super::stroke::InkStroke;

/// 单节点最多容纳的条目数，超出且未达深度上限时分裂。
const MAX_ITEMS: usize = 16;
/// 最大细分深度，防止退化输入无限分裂。
const MAX_DEPTH: u8 = 6;

/// 计算笔迹在屏幕空间的包围盒，已按线宽向外扩张半个线宽。
///
/// 点数不足 2 的笔迹不可见（渲染层同样跳过），返回 `None`。
pub fn stroke_bounds(stroke: &InkStroke) -> Option<Rect> {
    if stroke.points.len() < 2 {
        return None;
    }
    let (mut min_x, mut min_y) = (f32::MAX, f32::MAX);
    let (mut max_x, mut max_y) = (f32::MIN, f32::MIN);
    for &(x, y) in &stroke.points {
        if !x.is_finite() || !y.is_finite() {
            continue;
        }
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    if min_x > max_x || min_y > max_y {
        return None;
    }
    // 线宽会让实际覆盖范围超出几何点集，外扩半个线宽（下限 1px）避免剔除掉边缘。
    let pad = (stroke.thickness * 0.5).max(1.0);
    Some(Rect::from_min_max(Pos2::new(min_x, min_y), Pos2::new(max_x, max_y)).expand(pad))
}

struct Node {
    bounds: Rect,
    depth: u8,
    /// `(笔迹下标, 包围盒)`；跨子节点边界的条目滞留在此。
    items: Vec<(usize, Rect)>,
    children: Option<Box<[Node; 4]>>,
}

impl Node {
    fn new(bounds: Rect, depth: u8) -> Self {
        Self {
            bounds,
            depth,
            items: Vec::new(),
            children: None,
        }
    }

    fn split(&mut self) {
        let c = self.bounds.center();
        let min = self.bounds.min;
        let max = self.bounds.max;
        let d = self.depth + 1;
        self.children = Some(Box::new([
            Node::new(Rect::from_min_max(min, c), d),
            Node::new(
                Rect::from_min_max(Pos2::new(c.x, min.y), Pos2::new(max.x, c.y)),
                d,
            ),
            Node::new(
                Rect::from_min_max(Pos2::new(min.x, c.y), Pos2::new(c.x, max.y)),
                d,
            ),
            Node::new(Rect::from_min_max(c, max), d),
        ]));

        // 已有条目尽量下沉到能完整容纳它的子节点。
        let existing = std::mem::take(&mut self.items);
        for (idx, bbox) in existing {
            self.place(idx, bbox);
        }
    }

    /// 把条目放进能完整容纳它的子节点；放不下就留在本节点。
    fn place(&mut self, idx: usize, bbox: Rect) {
        if let Some(children) = self.children.as_mut() {
            for child in children.iter_mut() {
                if child.bounds.contains_rect(bbox) {
                    child.insert(idx, bbox);
                    return;
                }
            }
        }
        self.items.push((idx, bbox));
    }

    fn insert(&mut self, idx: usize, bbox: Rect) {
        if self.children.is_none() && self.items.len() >= MAX_ITEMS && self.depth < MAX_DEPTH {
            self.split();
        }
        self.place(idx, bbox);
    }

    fn query(&self, area: Rect, out: &mut Vec<usize>) {
        if !self.bounds.intersects(area) {
            return;
        }
        for (idx, bbox) in &self.items {
            if bbox.intersects(area) {
                out.push(*idx);
            }
        }
        if let Some(children) = self.children.as_ref() {
            for child in children.iter() {
                child.query(area, out);
            }
        }
    }
}

/// 笔迹包围盒的四叉树索引。下标对应构建时传入的 `&[InkStroke]` 切片位置。
#[derive(Default)]
pub struct Quadtree {
    root: Option<Node>,
    /// 已索引的条目数（不含被跳过的无效笔迹）。
    len: usize,
}

impl Quadtree {
    /// 依据当前笔迹集合整体重建索引。O(n)，仅在笔迹集合变更后调用。
    pub fn rebuild(&mut self, strokes: &[InkStroke]) {
        self.root = None;
        self.len = 0;

        // 先求全局包围盒作为根节点范围。
        let mut world: Option<Rect> = None;
        let mut entries: Vec<(usize, Rect)> = Vec::with_capacity(strokes.len());
        for (i, s) in strokes.iter().enumerate() {
            if let Some(b) = stroke_bounds(s) {
                world = Some(match world {
                    Some(w) => w.union(b),
                    None => b,
                });
                entries.push((i, b));
            }
        }

        let Some(world) = world else { return };
        // 稍微放大根范围，规避浮点边界导致的 contains_rect 判定抖动。
        let mut root = Node::new(world.expand(1.0), 0);
        for (idx, bbox) in entries {
            root.insert(idx, bbox);
        }
        self.len = root_count(&root);
        self.root = Some(root);
    }

    /// 查询与 `area` 相交的笔迹下标。结果顺序不保证，调用方若需稳定绘制顺序应排序。
    pub fn query(&self, area: Rect) -> Vec<usize> {
        let mut out = Vec::new();
        if let Some(root) = self.root.as_ref() {
            root.query(area, &mut out);
        }
        out
    }

    /// 查询某点半径 `radius` 邻域内的候选笔迹下标（橡皮命中测试用）。
    pub fn query_circle(&self, center: Pos2, radius: f32) -> Vec<usize> {
        self.query(Rect::from_center_size(center, Vec2::splat(radius * 2.0)))
    }

    /// 已索引的笔迹数量。
    ///
    /// 由宿主 crate 的性能监控面板消费；display 自身的 bin target 不使用，
    /// 故显式豁免 dead_code。
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.len
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

fn root_count(node: &Node) -> usize {
    let mut n = node.items.len();
    if let Some(children) = node.children.as_ref() {
        for c in children.iter() {
            n += root_count(c);
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotation::stroke::ToolType;

    fn stroke_at(x: f32, y: f32) -> InkStroke {
        let mut s = InkStroke::new(ToolType::Pen, [0, 0, 0], 255, 2.0);
        s.points.push((x, y));
        s.points.push((x + 5.0, y + 5.0));
        s
    }

    #[test]
    fn bounds_skips_degenerate_stroke() {
        let mut s = InkStroke::new(ToolType::Pen, [0, 0, 0], 255, 2.0);
        assert!(stroke_bounds(&s).is_none());
        s.points.push((1.0, 1.0));
        assert!(stroke_bounds(&s).is_none());
    }

    #[test]
    fn bounds_expands_by_half_thickness() {
        let mut s = InkStroke::new(ToolType::Pen, [0, 0, 0], 255, 10.0);
        s.points.push((0.0, 0.0));
        s.points.push((10.0, 10.0));
        let b = stroke_bounds(&s).unwrap();
        assert!(b.min.x <= -5.0 && b.max.x >= 15.0);
    }

    #[test]
    fn rebuild_indexes_all_valid_strokes() {
        let strokes: Vec<InkStroke> = (0..50)
            .map(|i| stroke_at(i as f32 * 20.0, i as f32 * 20.0))
            .collect();
        let mut qt = Quadtree::default();
        qt.rebuild(&strokes);
        assert_eq!(qt.len(), 50);
    }

    #[test]
    fn query_returns_only_overlapping_strokes() {
        // 左上角一条，右下角一条，相距很远。
        let strokes = vec![stroke_at(0.0, 0.0), stroke_at(1000.0, 1000.0)];
        let mut qt = Quadtree::default();
        qt.rebuild(&strokes);

        let hit = qt.query(Rect::from_min_max(
            Pos2::new(-10.0, -10.0),
            Pos2::new(50.0, 50.0),
        ));
        assert_eq!(hit, vec![0]);

        let hit2 = qt.query(Rect::from_min_max(
            Pos2::new(990.0, 990.0),
            Pos2::new(1050.0, 1050.0),
        ));
        assert_eq!(hit2, vec![1]);
    }

    #[test]
    fn query_circle_finds_nearby_stroke() {
        let strokes = vec![stroke_at(100.0, 100.0), stroke_at(800.0, 800.0)];
        let mut qt = Quadtree::default();
        qt.rebuild(&strokes);
        let hit = qt.query_circle(Pos2::new(102.0, 102.0), 8.0);
        assert!(hit.contains(&0));
        assert!(!hit.contains(&1));
    }

    #[test]
    fn empty_rebuild_yields_empty_index() {
        let mut qt = Quadtree::default();
        qt.rebuild(&[]);
        assert!(qt.is_empty());
        assert!(qt.query(Rect::EVERYTHING).is_empty());
    }

    #[test]
    fn deeply_clustered_strokes_do_not_lose_entries() {
        // 全部挤在同一小块区域，强制触发分裂与滞留逻辑。
        let strokes: Vec<InkStroke> = (0..200).map(|_| stroke_at(10.0, 10.0)).collect();
        let mut qt = Quadtree::default();
        qt.rebuild(&strokes);
        assert_eq!(qt.len(), 200);
        assert_eq!(qt.query(Rect::EVERYTHING).len(), 200);
    }
}
