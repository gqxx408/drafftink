//! 几何求解器 — 拓扑排序 + 增量更新
//!
//! 核心职责：
//! 1. 按 Kahn 算法对点定义进行拓扑排序
//! 2. 按依赖顺序求解每个点的具体坐标
//! 3. 使用 dirty 标记实现增量更新
//!
//! # 求解规则
//! - `Free` → 直接取 pos
//! - `Midpoint` → (a + b) / 2
//! - `OnLine` → start + t * (end - start)
//! - `OnCircle` → center + radius * (cos(θ), sin(θ))
//! - `Intersection` → 两条线段的交点
//! - `LineCircleIntersection` → 线与圆的交点（选择第一个或第二个）

use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

use crate::definitions::{GeometryDoc, Point2D, Point3D, PointDef};

/// 求解结果 — 存储所有已解析的点坐标
#[derive(Debug, Clone, Default)]
pub struct SolverContext {
    /// 2D 点坐标
    pub points_2d: HashMap<Uuid, Point2D>,
    /// 3D 点坐标
    pub points_3d: HashMap<Uuid, Point3D>,
}

impl SolverContext {
    /// 获取 2D 点坐标
    pub fn get_2d(&self, id: Uuid) -> Option<Point2D> {
        self.points_2d.get(&id).copied()
    }

    /// 获取 3D 点坐标
    pub fn get_3d(&self, id: Uuid) -> Option<Point3D> {
        self.points_3d.get(&id).copied()
    }
}

/// 几何求解器
///
/// 持有几何文档，通过 `solve()` 解析所有点的具体坐标。
/// 使用 dirty 标记避免不必要的重复计算。
pub struct GeometrySolver {
    /// 几何文档（定义）
    pub doc: GeometryDoc,
    /// 上次求解结果缓存
    pub cached: SolverContext,
    /// 是否需要重新求解
    dirty: bool,
}

impl GeometrySolver {
    /// 创建空求解器
    pub fn new() -> Self {
        Self {
            doc: GeometryDoc::new(),
            cached: SolverContext::default(),
            dirty: true,
        }
    }

    /// 从文档创建求解器
    pub fn from_doc(doc: GeometryDoc) -> Self {
        Self {
            doc,
            cached: SolverContext::default(),
            dirty: true,
        }
    }

    /// 标记为脏（需要重新求解）
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// 添加自由点
    pub fn add_free_point(&mut self, pos: Point2D) -> Uuid {
        let id = self.doc.add_free_point(pos);
        self.mark_dirty();
        id
    }

    /// 添加 3D 自由点
    pub fn add_free_point_3d(&mut self, pos: Point3D) -> Uuid {
        let id = self.doc.add_free_point_3d(pos);
        self.mark_dirty();
        id
    }

    /// 添加中点
    pub fn add_midpoint(&mut self, a: Uuid, b: Uuid) -> Uuid {
        let id = self.doc.add_midpoint(a, b);
        self.mark_dirty();
        id
    }

    /// 添加线段
    pub fn add_line(&mut self, start: Uuid, end: Uuid) -> anyhow::Result<Uuid> {
        // 验证引用有效性
        if !self.doc.points.contains_key(&start) {
            anyhow::bail!("起点 {start} 不存在");
        }
        if !self.doc.points.contains_key(&end) {
            anyhow::bail!("终点 {end} 不存在");
        }
        let id = self.doc.add_line(start, end);
        self.mark_dirty();
        Ok(id)
    }

    /// 添加圆
    pub fn add_circle(&mut self, center: Uuid, radius: f32) -> anyhow::Result<Uuid> {
        if !self.doc.points.contains_key(&center) {
            anyhow::bail!("圆心点 {center} 不存在");
        }
        if radius <= 0.0 {
            anyhow::bail!("半径必须为正数");
        }
        let id = self.doc.add_circle(center, radius);
        self.mark_dirty();
        Ok(id)
    }

    /// 添加立方体
    pub fn add_cube(&mut self, center: Uuid, size: f32) -> anyhow::Result<Uuid> {
        if !self.doc.points.contains_key(&center) {
            anyhow::bail!("中心点 {center} 不存在");
        }
        let id = self.doc.add_cube(center, size);
        self.mark_dirty();
        Ok(id)
    }

    /// 添加球体
    pub fn add_sphere(&mut self, center: Uuid, radius: f32) -> anyhow::Result<Uuid> {
        if !self.doc.points.contains_key(&center) {
            anyhow::bail!("中心点 {center} 不存在");
        }
        if radius <= 0.0 {
            anyhow::bail!("半径必须为正数");
        }
        let id = self.doc.add_sphere(center, radius);
        self.mark_dirty();
        Ok(id)
    }

    /// 更新自由点位置（拖拽时调用）
    pub fn update_free_point(&mut self, id: Uuid, new_pos: Point2D) {
        self.doc.update_free_point(id, new_pos);
        self.mark_dirty();
    }

    /// 删除元素
    pub fn remove_element(&mut self, id: Uuid) {
        self.doc.remove_element(id);
        self.mark_dirty();
    }

    /// 求解 — 按拓扑顺序解析所有点坐标
    ///
    /// 使用 Kahn 算法进行拓扑排序，确保依赖在前、被依赖在后。
    pub fn solve(&mut self) -> &SolverContext {
        if !self.dirty {
            return &self.cached;
        }

        let mut ctx = SolverContext::default();

        // ── 拓扑排序（Kahn 算法）──
        let order = self.topological_sort();

        // ── 按拓扑顺序求解每个点 ──
        for id in order {
            if let Some(def) = self.doc.points.get(&id) {
                if let Err(e) = self.solve_point(id, def, &mut ctx) {
                    log::warn!("求解点 {id} 失败: {e}");
                }
            }
        }

        self.cached = ctx;
        self.dirty = false;
        &self.cached
    }

    /// Kahn 拓扑排序
    ///
    /// 构建依赖图，计算入度，按 BFS 顺序输出。
    fn topological_sort(&self) -> Vec<Uuid> {
        let mut in_degree: HashMap<Uuid, usize> = HashMap::new();
        let mut adj: HashMap<Uuid, Vec<Uuid>> = HashMap::new();

        // 初始化所有点的入度为 0
        for &id in self.doc.points.keys() {
            in_degree.insert(id, 0);
            adj.insert(id, Vec::new());
        }

        // 构建依赖边：如果 A 依赖 B，则 B → A（B 在 A 之前求解）
        for (&id, def) in &self.doc.points {
            for dep in def.dependencies() {
                // dep 可以是点 ID 或线/圆 ID
                // 如果 dep 是一个点，直接加边
                if self.doc.points.contains_key(&dep) {
                    if let Some(v) = adj.get_mut(&dep) {
                        v.push(id)
                    }
                    *in_degree.get_mut(&id).unwrap_or(&mut 0) += 1;
                }
                // 如果 dep 是线/圆，我们需要找到线/圆引用的点
                else if let Some(line) = self.doc.lines.get(&dep) {
                    for &ref_id in &[line.start, line.end] {
                        if ref_id != id && self.doc.points.contains_key(&ref_id) {
                            if let Some(v) = adj.get_mut(&ref_id) {
                                v.push(id)
                            }
                            *in_degree.get_mut(&id).unwrap_or(&mut 0) += 1;
                        }
                    }
                } else if let Some(circle) = self.doc.circles.get(&dep) {
                    if circle.center != id && self.doc.points.contains_key(&circle.center) {
                        if let Some(v) = adj.get_mut(&circle.center) {
                            v.push(id)
                        }
                        *in_degree.get_mut(&id).unwrap_or(&mut 0) += 1;
                    }
                }
            }
        }

        // Kahn BFS
        let mut queue: VecDeque<Uuid> = VecDeque::new();
        for (&id, &deg) in &in_degree {
            if deg == 0 {
                queue.push_back(id);
            }
        }

        let mut result = Vec::with_capacity(self.doc.points.len());
        let mut visited: HashSet<Uuid> = HashSet::new();

        while let Some(id) = queue.pop_front() {
            if visited.contains(&id) {
                continue;
            }
            visited.insert(id);
            result.push(id);

            if let Some(neighbors) = adj.get(&id) {
                for &neighbor in neighbors {
                    if let Some(deg) = in_degree.get_mut(&neighbor) {
                        *deg = deg.saturating_sub(1);
                        if *deg == 0 && !visited.contains(&neighbor) {
                            queue.push_back(neighbor);
                        }
                    }
                }
            }
        }

        // 处理循环依赖（理论上不应该出现，但做防御）
        for &id in self.doc.points.keys() {
            if !visited.contains(&id) {
                log::warn!("点 {id} 存在循环依赖，强制求解");
                result.push(id);
            }
        }

        result
    }

    /// 求解单个点
    fn solve_point(&self, id: Uuid, def: &PointDef, ctx: &mut SolverContext) -> anyhow::Result<()> {
        match def {
            PointDef::Free { pos } => {
                ctx.points_2d.insert(id, *pos);
            }
            PointDef::Free3D { pos } => {
                ctx.points_3d.insert(id, *pos);
            }
            PointDef::Midpoint { a, b } => {
                let pa = ctx
                    .get_2d(*a)
                    .ok_or_else(|| anyhow::anyhow!("依赖点 {a} 未求解"))?;
                let pb = ctx
                    .get_2d(*b)
                    .ok_or_else(|| anyhow::anyhow!("依赖点 {b} 未求解"))?;
                let mid = (pa + pb) * 0.5;
                ctx.points_2d.insert(id, mid);
            }
            PointDef::OnLine { line, t } => {
                let line_def = self
                    .doc
                    .lines
                    .get(line)
                    .ok_or_else(|| anyhow::anyhow!("线 {line} 不存在"))?;
                let start = ctx
                    .get_2d(line_def.start)
                    .ok_or_else(|| anyhow::anyhow!("线起点未求解"))?;
                let end = ctx
                    .get_2d(line_def.end)
                    .ok_or_else(|| anyhow::anyhow!("线终点未求解"))?;
                let pos = start + (end - start) * *t;
                ctx.points_2d.insert(id, pos);
            }
            PointDef::OnCircle { circle, angle } => {
                let circle_def = self
                    .doc
                    .circles
                    .get(circle)
                    .ok_or_else(|| anyhow::anyhow!("圆 {circle} 不存在"))?;
                let center = ctx
                    .get_2d(circle_def.center)
                    .ok_or_else(|| anyhow::anyhow!("圆心未求解"))?;
                let offset = Point2D::new(
                    circle_def.radius * angle.cos(),
                    circle_def.radius * angle.sin(),
                );
                ctx.points_2d.insert(id, center + offset);
            }
            PointDef::Intersection { line_a, line_b } => {
                let la = self
                    .doc
                    .lines
                    .get(line_a)
                    .ok_or_else(|| anyhow::anyhow!("线 {line_a} 不存在"))?;
                let lb = self
                    .doc
                    .lines
                    .get(line_b)
                    .ok_or_else(|| anyhow::anyhow!("线 {line_b} 不存在"))?;

                let p1 = ctx
                    .get_2d(la.start)
                    .ok_or_else(|| anyhow::anyhow!("线A起点未求解"))?;
                let p2 = ctx
                    .get_2d(la.end)
                    .ok_or_else(|| anyhow::anyhow!("线A终点未求解"))?;
                let p3 = ctx
                    .get_2d(lb.start)
                    .ok_or_else(|| anyhow::anyhow!("线B起点未求解"))?;
                let p4 = ctx
                    .get_2d(lb.end)
                    .ok_or_else(|| anyhow::anyhow!("线B终点未求解"))?;

                if let Some(pt) = line_line_intersection(p1, p2, p3, p4) {
                    ctx.points_2d.insert(id, pt);
                } else {
                    log::warn!("线 {line_a} 和 {line_b} 平行，无交点");
                }
            }
            PointDef::LineCircleIntersection {
                line,
                circle,
                which,
            } => {
                let line_def = self
                    .doc
                    .lines
                    .get(line)
                    .ok_or_else(|| anyhow::anyhow!("线 {line} 不存在"))?;
                let circle_def = self
                    .doc
                    .circles
                    .get(circle)
                    .ok_or_else(|| anyhow::anyhow!("圆 {circle} 不存在"))?;

                let p1 = ctx
                    .get_2d(line_def.start)
                    .ok_or_else(|| anyhow::anyhow!("线起点未求解"))?;
                let p2 = ctx
                    .get_2d(line_def.end)
                    .ok_or_else(|| anyhow::anyhow!("线终点未求解"))?;
                let center = ctx
                    .get_2d(circle_def.center)
                    .ok_or_else(|| anyhow::anyhow!("圆心未求解"))?;

                let intersections = line_circle_intersection(p1, p2, center, circle_def.radius);
                if let Some(&pt) = intersections.get(if *which { 0 } else { 1 }) {
                    ctx.points_2d.insert(id, pt);
                } else {
                    log::warn!("线 {line} 与圆 {circle} 无交点或交点不足");
                }
            }
        }
        Ok(())
    }
}

impl Default for GeometrySolver {
    fn default() -> Self {
        Self::new()
    }
}

// ── 数学辅助函数 ────────────────────────────────────────────────

/// 两线段交点
///
/// 使用参数方程求解：
/// P = p1 + s * (p2 - p1) = p3 + t * (p4 - p3)
fn line_line_intersection(p1: Point2D, p2: Point2D, p3: Point2D, p4: Point2D) -> Option<Point2D> {
    let d1 = p2 - p1;
    let d2 = p4 - p3;

    let denom = d1.x * d2.y - d1.y * d2.x;
    if denom.abs() < 1e-10 {
        return None; // 平行
    }

    let diff = p3 - p1;
    let s = (diff.x * d2.y - diff.y * d2.x) / denom;

    Some(p1 + d1 * s)
}

/// 线段与圆的交点
///
/// 线段参数方程 P = p1 + t * (p2 - p1)
/// 圆方程 |P - center|² = r²
fn line_circle_intersection(
    p1: Point2D,
    p2: Point2D,
    center: Point2D,
    radius: f32,
) -> Vec<Point2D> {
    let d = p2 - p1;
    let f = p1 - center;

    let a = d.dot(&d);
    let b = 2.0 * f.dot(&d);
    let c = f.dot(&f) - radius * radius;

    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 || a.abs() < 1e-10 {
        return Vec::new();
    }

    let sqrt_disc = discriminant.sqrt();
    let t1 = (-b - sqrt_disc) / (2.0 * a);
    let t2 = (-b + sqrt_disc) / (2.0 * a);

    let mut result = Vec::new();
    if (0.0..=1.0).contains(&t1) {
        result.push(p1 + d * t1);
    }
    if (0.0..=1.0).contains(&t2) && (t2 - t1).abs() > 1e-10 {
        result.push(p1 + d * t2);
    }
    result
}

// ── 测试 ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_solve_free_point() {
        let mut solver = GeometrySolver::new();
        let id = solver.add_free_point(Point2D::new(10.0, 20.0));
        let ctx = solver.solve();

        assert_eq!(ctx.get_2d(id), Some(Point2D::new(10.0, 20.0)));
    }

    #[test]
    fn test_solve_midpoint() {
        let mut solver = GeometrySolver::new();
        let a = solver.add_free_point(Point2D::new(0.0, 0.0));
        let b = solver.add_free_point(Point2D::new(10.0, 20.0));
        let mid = solver.add_midpoint(a, b);

        let ctx = solver.solve();
        assert_eq!(ctx.get_2d(mid), Some(Point2D::new(5.0, 10.0)));
    }

    #[test]
    fn test_solve_on_line() {
        let mut solver = GeometrySolver::new();
        let a = solver.add_free_point(Point2D::new(0.0, 0.0));
        let b = solver.add_free_point(Point2D::new(10.0, 0.0));
        let line_id = solver.add_line(a, b).unwrap();

        // 在线上 25% 处
        let p_id = {
            let id = uuid::Uuid::new_v4();
            solver.doc.points.insert(
                id,
                PointDef::OnLine {
                    line: line_id,
                    t: 0.25,
                },
            );
            solver.mark_dirty();
            id
        };

        let ctx = solver.solve();
        assert_eq!(ctx.get_2d(p_id), Some(Point2D::new(2.5, 0.0)));
    }

    #[test]
    fn test_solve_on_circle() {
        let mut solver = GeometrySolver::new();
        let center = solver.add_free_point(Point2D::new(5.0, 5.0));
        let circle_id = solver.add_circle(center, 3.0).unwrap();

        let p_id = {
            let id = uuid::Uuid::new_v4();
            solver.doc.points.insert(
                id,
                PointDef::OnCircle {
                    circle: circle_id,
                    angle: 0.0,
                },
            );
            solver.mark_dirty();
            id
        };

        let ctx = solver.solve();
        assert_eq!(ctx.get_2d(p_id), Some(Point2D::new(8.0, 5.0)));
    }

    #[test]
    fn test_solve_intersection() {
        let mut solver = GeometrySolver::new();
        let p1 = solver.add_free_point(Point2D::new(0.0, 0.0));
        let p2 = solver.add_free_point(Point2D::new(10.0, 10.0));
        let p3 = solver.add_free_point(Point2D::new(0.0, 10.0));
        let p4 = solver.add_free_point(Point2D::new(10.0, 0.0));

        let la = solver.add_line(p1, p2).unwrap();
        let lb = solver.add_line(p3, p4).unwrap();

        let ix_id = {
            let id = uuid::Uuid::new_v4();
            solver.doc.points.insert(
                id,
                PointDef::Intersection {
                    line_a: la,
                    line_b: lb,
                },
            );
            solver.mark_dirty();
            id
        };

        let ctx = solver.solve();
        let pt = ctx.get_2d(ix_id).expect("交点应存在");
        assert!((pt.x - 5.0).abs() < 0.01);
        assert!((pt.y - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_drag_updates() {
        let mut solver = GeometrySolver::new();
        let a = solver.add_free_point(Point2D::new(0.0, 0.0));
        let b = solver.add_free_point(Point2D::new(10.0, 0.0));
        let mid = solver.add_midpoint(a, b);

        let ctx = solver.solve();
        assert_eq!(ctx.get_2d(mid), Some(Point2D::new(5.0, 0.0)));

        // 拖拽 a 到 (0, 10)
        solver.update_free_point(a, Point2D::new(0.0, 10.0));
        let ctx = solver.solve();
        assert_eq!(ctx.get_2d(mid), Some(Point2D::new(5.0, 5.0)));
    }

    #[test]
    fn test_line_line_intersection_math() {
        let p1 = Point2D::new(0.0, 0.0);
        let p2 = Point2D::new(10.0, 10.0);
        let p3 = Point2D::new(0.0, 10.0);
        let p4 = Point2D::new(10.0, 0.0);

        let pt = line_line_intersection(p1, p2, p3, p4);
        assert!(pt.is_some());
        let pt = pt.unwrap();
        assert!((pt.x - 5.0).abs() < 0.001);
        assert!((pt.y - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_parallel_lines_no_intersection() {
        let p1 = Point2D::new(0.0, 0.0);
        let p2 = Point2D::new(10.0, 0.0);
        let p3 = Point2D::new(0.0, 5.0);
        let p4 = Point2D::new(10.0, 5.0);

        assert!(line_line_intersection(p1, p2, p3, p4).is_none());
    }

    #[test]
    fn test_caching() {
        let mut solver = GeometrySolver::new();
        let _id = solver.add_free_point(Point2D::new(1.0, 2.0));

        let ctx1 = solver.solve();
        let ptr1 = ctx1 as *const SolverContext;

        // 无变化，应返回缓存
        let ctx2 = solver.solve();
        let ptr2 = ctx2 as *const SolverContext;

        assert_eq!(ptr1, ptr2);
    }
}
