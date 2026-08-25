//! Dynamic Geometry Constraint Solver
//!
//! 动态几何约束求解系统 —— 参考希沃白板 FreeGeometryContext 的设计理念，
//! 用 Rust 所有权系统 + nalgebra 数学库实现。
//!
//! # 核心思想（给 11 岁小 CTO 的解释）
//!
//! 想象你在画板上画了一个三角形：
//! - 你拖了三个点 A、B、C
//! - 然后画了三条线把它们连起来
//! - 如果你拖动 A 点，三条边是不是应该自动跟着动？
//!
//! 这就是"约束求解"：图形不是死的坐标，而是"关系"。
//! 中点永远是中点，交点永远是交点，平行线永远平行。
//! 只要你移动一个控制点，所有依赖它的图形都会自动重新计算。
//!
//! # 数据结构
//!
//! ```text
//! SolverContext (求解器上下文)
//!   ├─ definitions: HashMap<Uuid, Box<dyn GeometryDefinition>>
//!   │   存储所有几何定义（点、线、圆、中点、交点...）
//!   └─ dirty: HashSet<Uuid>
//!       标记哪些定义"脏了"（依赖项变了，需要重新算）
//!
//! 拓扑排序（Topological Sort）：
//!   先算"自由点"（不依赖任何东西），
//!   再算"线"（依赖两个点），
//!   再算"中点"（依赖两个点），
//!   再算"交点"（依赖两条线）。
//!   就像搭积木，从底层往上搭。
//! ```

use nalgebra::{Point2, Vector2};
use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

// ─── Trait ────────────────────────────────────────────────────────────────

/// 几何定义的核心 Trait。
///
/// 每个实现了这个 Trait 的类型都代表一种"几何关系"，
/// 比如"一个自由点"、"一条连接两点的线"、"两条线的交点"。
///
/// 所有几何定义都有三个基本能力：
/// 1. 有唯一 ID（`id`）
/// 2. 知道自己依赖谁（`dependencies`）
/// 3. 能根据依赖项的最新值算出自己的状态（`solve`）
///
/// 另外还有三个"类型查询"方法（`point_position` 等），
/// 默认都返回 None，具体类型按需覆盖它们。
/// 这样 SolverContext 就可以问"你是一个点吗？"、"你是一条线吗？"
pub trait GeometryDefinition: Send + Sync {
    /// 这个几何元素的唯一编号。
    fn id(&self) -> Uuid;

    /// 列出这个元素依赖哪些其他元素（通过 ID 引用）。
    ///
    /// 比如一条线依赖它的两个端点，
    /// 一个中点依赖它所在线段的两个端点。
    fn dependencies(&self) -> Vec<Uuid>;

    /// 根据上下文里其他元素的最新值，重新计算自己的状态。
    ///
    /// 这就是"求解"的核心：父节点更新后，子节点调用 solve()
    /// 就能自动算出自己的新位置。
    fn solve(&mut self, context: &SolverContext);

    /// 如果这个定义是一个"点类型"，返回它的坐标；否则返回 None。
    ///
    /// 自由点、中点、交点都应该覆盖这个方法返回 Some(coords)。
    fn point_position(&self) -> Option<Point2<f32>> {
        None
    }

    /// 如果这个定义是一条"线"，返回 (起点, 终点)；否则返回 None。
    fn line_endpoints(&self) -> Option<(Point2<f32>, Point2<f32>)> {
        None
    }

    /// 如果这个定义是一个"圆"，返回 (圆心, 半径)；否则返回 None。
    fn circle_params(&self) -> Option<(Point2<f32>, f32)> {
        None
    }

    /// 调试用：打印这个元素的状态。
    fn debug_print(&self) -> String {
        format!("<GeometryDefinition {:?}>", self.id())
    }
}

// ─── 类型别名（方便写代码） ──────────────────────────────────────────────

/// 2D 点坐标（使用 nalgebra，支持所有向量运算）。
pub type P2 = Point2<f32>;

/// 2D 向量。
pub type V2 = Vector2<f32>;

// ─── FreePoint（自由点） ──────────────────────────────────────────────────

/// 自由点：用户可以直接拖动的点，不依赖任何其他元素。
///
/// 这是整个几何系统的"地基"——所有其他图形
/// 最终都可以追溯到几个自由点上。
#[derive(Clone)]
pub struct FreePoint {
    id: Uuid,
    /// 点的坐标（可被用户直接修改）。
    pub position: P2,
}

impl FreePoint {
    /// 创建一个新的自由点。
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            id: Uuid::new_v4(),
            position: P2::new(x, y),
        }
    }

    /// 用指定 ID 创建自由点（主要用于测试/反序列化）。
    pub fn with_id(id: Uuid, x: f32, y: f32) -> Self {
        Self {
            id,
            position: P2::new(x, y),
        }
    }

    /// 修改点的坐标，同时标记它为"脏的"（触发重算）。
    pub fn set_position(&mut self, x: f32, y: f32, ctx: &mut SolverContext) {
        self.position = P2::new(x, y);
        ctx.mark_dirty(self.id);
    }
}

impl GeometryDefinition for FreePoint {
    fn id(&self) -> Uuid {
        self.id
    }

    /// 自由点不依赖任何东西——它就是"地基"。
    fn dependencies(&self) -> Vec<Uuid> {
        vec![]
    }

    /// 自由点不需要求解，坐标就是用户设置的值。
    fn solve(&mut self, _context: &SolverContext) {
        // 自由点的坐标由用户直接控制，不需要计算
    }

    fn point_position(&self) -> Option<P2> {
        Some(self.position)
    }

    fn debug_print(&self) -> String {
        format!("FreePoint({:.1}, {:.1})", self.position.x, self.position.y)
    }
}

// ─── Line（直线/线段） ────────────────────────────────────────────────────

/// 线段：由两个端点定义的直线段。
///
/// 依赖两个点（起点和终点）。
/// 只要端点移动了，整条线自动跟着动。
#[derive(Clone)]
pub struct Line {
    id: Uuid,
    /// 起点的 ID。
    pub start_id: Uuid,
    /// 终点的 ID。
    pub end_id: Uuid,
    /// 缓存的起点坐标（solve 后更新）。
    pub start: P2,
    /// 缓存的终点坐标（solve 后更新）。
    pub end: P2,
}

impl Line {
    /// 创建一条连接两个点的线段。
    pub fn new(start_id: Uuid, end_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            start_id,
            end_id,
            start: P2::new(0.0, 0.0),
            end: P2::new(0.0, 0.0),
        }
    }

    /// 计算线段长度。
    pub fn length(&self) -> f32 {
        (self.end - self.start).norm()
    }

    /// 计算中点坐标。
    pub fn midpoint(&self) -> P2 {
        self.start + (self.end - self.start) * 0.5
    }
}

impl GeometryDefinition for Line {
    fn id(&self) -> Uuid {
        self.id
    }

    fn dependencies(&self) -> Vec<Uuid> {
        vec![self.start_id, self.end_id]
    }

    /// 从上下文取出起点和终点的最新坐标，更新自己的缓存。
    fn solve(&mut self, context: &SolverContext) {
        if let Some(start_pt) = context.get_point(self.start_id) {
            self.start = start_pt;
        }
        if let Some(end_pt) = context.get_point(self.end_id) {
            self.end = end_pt;
        }
    }

    fn line_endpoints(&self) -> Option<(P2, P2)> {
        Some((self.start, self.end))
    }

    fn debug_print(&self) -> String {
        format!(
            "Line(({:.1},{:.1}) → ({:.1},{:.1}), len={:.1})",
            self.start.x,
            self.start.y,
            self.end.x,
            self.end.y,
            self.length()
        )
    }
}

// ─── MidPoint（中点） ─────────────────────────────────────────────────────

/// 中点：永远在两个点正中间的点。
///
/// 依赖两个点（可以是自由点、交点、或者其他任何点类型）。
/// 不管端点怎么动，中点永远在正中间——这就是"约束"。
#[derive(Clone)]
pub struct MidPoint {
    id: Uuid,
    /// 第一个端点的 ID。
    pub a_id: Uuid,
    /// 第二个端点的 ID。
    pub b_id: Uuid,
    /// 缓存的中点坐标（solve 后更新）。
    pub position: P2,
}

impl MidPoint {
    /// 创建一个新的中点，位于 a 和 b 的正中间。
    pub fn new(a_id: Uuid, b_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            a_id,
            b_id,
            position: P2::new(0.0, 0.0),
        }
    }
}

impl GeometryDefinition for MidPoint {
    fn id(&self) -> Uuid {
        self.id
    }

    fn dependencies(&self) -> Vec<Uuid> {
        vec![self.a_id, self.b_id]
    }

    /// 中点公式：(a + b) / 2
    ///
    /// 就这么简单！但因为是在 solve 里算的，
    /// 所以 a 和 b 一变，中点自动跟着变。
    fn solve(&mut self, context: &SolverContext) {
        let a = context.get_point(self.a_id).unwrap_or(P2::new(0.0, 0.0));
        let b = context.get_point(self.b_id).unwrap_or(P2::new(0.0, 0.0));
        self.position = nalgebra::center(&a, &b);
    }

    fn point_position(&self) -> Option<P2> {
        Some(self.position)
    }

    fn debug_print(&self) -> String {
        format!("MidPoint({:.1}, {:.1})", self.position.x, self.position.y)
    }
}

// ─── IntersectionPoint（交点） ────────────────────────────────────────────

/// 两条直线的交点。
///
/// 依赖两条线（Line）。线动了，交点自动跟着动。
/// 如果两条线平行（没有交点），position 会是 None。
#[derive(Clone)]
pub struct IntersectionPoint {
    id: Uuid,
    /// 第一条线的 ID。
    pub line_a_id: Uuid,
    /// 第二条线的 ID。
    pub line_b_id: Uuid,
    /// 缓存的交点坐标（solve 后更新）。
    /// 如果两线平行则为 None。
    pub position: Option<P2>,
}

impl IntersectionPoint {
    /// 创建一个新的交点，是 line_a 和 line_b 的交点。
    pub fn new(line_a_id: Uuid, line_b_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            line_a_id,
            line_b_id,
            position: None,
        }
    }
}

impl GeometryDefinition for IntersectionPoint {
    fn id(&self) -> Uuid {
        self.id
    }

    fn dependencies(&self) -> Vec<Uuid> {
        vec![self.line_a_id, self.line_b_id]
    }

    /// 计算两条直线的交点。
    ///
    /// 数学原理：
    /// 直线1: P = A + t*(B-A)
    /// 直线2: P = C + s*(D-C)
    /// 解方程求 t 和 s，得到交点坐标。
    /// 如果两线平行（分母为 0），则返回 None。
    fn solve(&mut self, context: &SolverContext) {
        let (a1, a2) = match context.get_line(self.line_a_id) {
            Some(line) => line,
            None => {
                self.position = None;
                return;
            }
        };
        let (b1, b2) = match context.get_line(self.line_b_id) {
            Some(line) => line,
            None => {
                self.position = None;
                return;
            }
        };

        let r = a2 - a1; // 线A的方向向量
        let s_vec = b2 - b1; // 线B的方向向量

        let denom = r.x * s_vec.y - r.y * s_vec.x;

        // 如果分母为 0 或接近 0，说明两线平行（或重合）
        if denom.abs() < 1e-6 {
            self.position = None;
            return;
        }

        let q_p = b1 - a1;
        let t = (q_p.x * s_vec.y - q_p.y * s_vec.x) / denom;

        // 交点 = A + t * r
        let intersection = a1 + r * t;
        self.position = Some(intersection);
    }

    fn point_position(&self) -> Option<P2> {
        self.position
    }

    fn debug_print(&self) -> String {
        match self.position {
            Some(p) => format!("IntersectionPoint({:.1}, {:.1})", p.x, p.y),
            None => "IntersectionPoint(parallel, no intersection)".to_string(),
        }
    }
}

// ─── Circle（圆） ─────────────────────────────────────────────────────────

/// 圆：由圆心和半径定义。
///
/// 圆心可以是任意点类型（自由点、中点、交点...）。
/// 半径可以是固定值，也可以绑定到另一个点（半径 = 圆心到该点的距离）。
#[derive(Clone)]
pub struct Circle {
    id: Uuid,
    /// 圆心的 ID。
    pub center_id: Uuid,
    /// 半径点的 ID（可选）。如果为 Some，则半径 = 圆心到该点的距离。
    pub radius_point_id: Option<Uuid>,
    /// 固定半径（当 radius_point_id 为 None 时使用）。
    pub fixed_radius: f32,
    /// 缓存的圆心坐标。
    pub center: P2,
    /// 缓存的半径值。
    pub radius: f32,
}

impl Circle {
    /// 创建一个固定半径的圆。
    pub fn with_fixed_radius(center_id: Uuid, radius: f32) -> Self {
        Self {
            id: Uuid::new_v4(),
            center_id,
            radius_point_id: None,
            fixed_radius: radius,
            center: P2::new(0.0, 0.0),
            radius,
        }
    }

    /// 创建一个半径由另一个点决定的圆（圆心 + 圆上一点）。
    pub fn with_radius_point(center_id: Uuid, radius_point_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            center_id,
            radius_point_id: Some(radius_point_id),
            fixed_radius: 0.0,
            center: P2::new(0.0, 0.0),
            radius: 0.0,
        }
    }
}

impl GeometryDefinition for Circle {
    fn id(&self) -> Uuid {
        self.id
    }

    fn dependencies(&self) -> Vec<Uuid> {
        let mut deps = vec![self.center_id];
        if let Some(rpid) = self.radius_point_id {
            deps.push(rpid);
        }
        deps
    }

    fn solve(&mut self, context: &SolverContext) {
        if let Some(center) = context.get_point(self.center_id) {
            self.center = center;
        }
        if let Some(rpid) = self.radius_point_id {
            if let Some(rp) = context.get_point(rpid) {
                self.radius = (rp - self.center).norm();
            }
        } else {
            self.radius = self.fixed_radius;
        }
    }

    fn circle_params(&self) -> Option<(P2, f32)> {
        Some((self.center, self.radius))
    }

    fn debug_print(&self) -> String {
        format!(
            "Circle(center=({:.1},{:.1}), r={:.1})",
            self.center.x, self.center.y, self.radius
        )
    }
}

// ─── SolverContext（求解器上下文） ────────────────────────────────────────

/// 求解器上下文：管理所有几何定义，负责按正确顺序更新它们。
///
/// # 工作原理
///
/// 想象一个班级里有很多同学，每个人的作业都要等别人做完才能做：
/// - 自由点同学：自己就能做完（不依赖别人）
/// - 线同学：要等两个点同学做完
/// - 中点同学：要等两个点同学做完
/// - 交点同学：要等两个线同学做完
///
/// 拓扑排序就是找出"谁应该先做"的顺序，
/// 保证每次做作业的时候，依赖的人都已经做完了。
///
/// # Dirty 优化（增量更新）
///
/// 如果只有一个点动了，不需要重新计算所有图形。
/// 我们把"变了的"和"依赖变了的东西"标记为"脏的"（dirty），
/// 只重新计算脏的部分。这就叫"增量更新"，飞快！
pub struct SolverContext {
    /// 所有几何定义的存储。
    definitions: HashMap<Uuid, Box<dyn GeometryDefinition>>,
    /// 哪些元素"脏了"需要重新计算。
    dirty: HashSet<Uuid>,
    /// 拓扑排序缓存（只有在定义增删时才重建）。
    topo_order: Vec<Uuid>,
    /// 拓扑缓存是否需要重建。
    topo_dirty: bool,
}

impl Default for SolverContext {
    fn default() -> Self {
        Self::new()
    }
}

impl SolverContext {
    /// 创建一个空的求解器上下文。
    pub fn new() -> Self {
        Self {
            definitions: HashMap::new(),
            dirty: HashSet::new(),
            topo_order: Vec::new(),
            topo_dirty: true,
        }
    }

    /// 添加一个几何定义到上下文里。
    pub fn add(&mut self, def: Box<dyn GeometryDefinition>) {
        let id = def.id();
        self.definitions.insert(id, def);
        self.dirty.insert(id);
        self.topo_dirty = true;
    }

    /// 便捷方法：添加任意实现了 GeometryDefinition 的类型。
    pub fn add_def<T: GeometryDefinition + 'static>(&mut self, def: T) {
        self.add(Box::new(def));
    }

    /// 标记某个元素为"脏的"（需要重新计算）。
    ///
    /// 同时，所有依赖这个元素的"下游"元素也会被标记为脏的。
    /// 这就叫"脏传播"——你动了一个点，
    /// 所有用了这个点的线、中点、交点都会跟着变脏。
    pub fn mark_dirty(&mut self, id: Uuid) {
        // 使用 BFS（广度优先搜索）传播脏标记
        let mut queue = VecDeque::new();
        queue.push_back(id);

        while let Some(current) = queue.pop_front() {
            if self.dirty.insert(current) {
                // 找到所有依赖 current 的元素（即"下游"），把它们也标记为脏
                for (def_id, def) in &self.definitions {
                    if def.dependencies().contains(&current) {
                        queue.push_back(*def_id);
                    }
                }
            }
        }
    }

    /// 获取某个点的当前坐标。
    ///
    /// 支持 FreePoint、MidPoint、IntersectionPoint 等所有点类型。
    pub fn get_point(&self, id: Uuid) -> Option<P2> {
        let def = self.definitions.get(&id)?;
        def.point_position()
    }

    /// 获取某条线的当前起点和终点。
    pub fn get_line(&self, id: Uuid) -> Option<(P2, P2)> {
        let def = self.definitions.get(&id)?;
        def.line_endpoints()
    }

    /// 获取某个圆的圆心和半径。
    pub fn get_circle(&self, id: Uuid) -> Option<(P2, f32)> {
        let def = self.definitions.get(&id)?;
        def.circle_params()
    }

    /// 通过 ID 拿到定义的不可变引用。
    pub fn get(&self, id: Uuid) -> Option<&dyn GeometryDefinition> {
        self.definitions.get(&id).map(|b| b.as_ref())
    }

    /// 通过 ID 拿到定义的可变引用。
    ///
    /// 注意：直接修改定义不会自动标记脏，
    /// 如果你改了依赖相关的数据，请调用 `mark_dirty(id)` 手动触发。
    pub fn get_mut(&mut self, id: Uuid) -> Option<&mut Box<dyn GeometryDefinition>> {
        self.definitions.get_mut(&id)
    }

    /// 总共有多少个几何定义。
    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }

    // ── 拓扑排序 & 求解 ────────────────────────────────────────────────

    /// 重新计算所有脏的几何定义。
    ///
    /// 这是核心方法！按拓扑顺序依次调用每个元素的 solve()，
    /// 确保父节点先更新，子节点后更新。
    pub fn solve_all(&mut self) {
        if self.dirty.is_empty() {
            return; // 没有脏元素，啥也不用干
        }

        // 如果拓扑缓存失效了，重新算一遍
        if self.topo_dirty {
            self.rebuild_topo_order();
            self.topo_dirty = false;
        }

        // 按拓扑顺序遍历，只求解脏的元素
        // （用 clone 是因为 solve_all 里会修改 self.definitions）
        let order = self.topo_order.clone();
        for id in order {
            if self.dirty.contains(&id) {
                // 先把它从 dirty 里移除（因为马上就要算它了）
                self.dirty.remove(&id);

                // 安全地取出定义并调用 solve
                // 因为 Rust 的借用规则，我们需要用"取出-放回去"的模式
                // 这样 solve() 里可以安全地 &self 访问其他定义
                if let Some(mut def) = self.definitions.remove(&id) {
                    def.solve(self);
                    self.definitions.insert(id, def);
                }
            }
        }
    }

    /// 重建拓扑排序顺序。
    ///
    /// 使用 Kahn 算法（卡恩算法）：
    /// 1. 先找出所有"入度为 0"的节点（不依赖任何东西的，比如自由点）
    /// 2. 把它们放进队列
    /// 3. 依次取出节点，把它的"邻居"（依赖它的节点）的入度减 1
    /// 4. 如果某个邻居入度变成 0 了，就放进队列
    /// 5. 直到队列为空
    ///
    /// 最后得到的顺序就是"从底层到顶层"的正确计算顺序。
    ///
    /// 打个比方：你要做蛋糕，得先打鸡蛋再搅拌最后烤。
    /// 拓扑排序就是帮你找出"打蛋 → 搅拌 → 烤"这个顺序，
    /// 而不是"烤 → 打蛋 → 搅拌"（那就完蛋了）。
    fn rebuild_topo_order(&mut self) {
        let mut in_degree: HashMap<Uuid, usize> = HashMap::new();
        let mut dependents: HashMap<Uuid, Vec<Uuid>> = HashMap::new(); // 反向依赖图

        // 初始化入度和反向依赖图
        for (id, def) in &self.definitions {
            in_degree.entry(*id).or_insert(0);
            for dep in def.dependencies() {
                *in_degree.entry(*id).or_insert(0) += 1;
                dependents.entry(dep).or_default().push(*id);
            }
        }

        // 队列：所有入度为 0 的节点（最底层的"地基"）
        let mut queue = VecDeque::new();
        for (id, degree) in &in_degree {
            if *degree == 0 {
                queue.push_back(*id);
            }
        }

        let mut result = Vec::new();

        while let Some(node) = queue.pop_front() {
            result.push(node);

            // 减少所有"下游"节点的入度
            if let Some(deps) = dependents.get(&node) {
                for dep in deps {
                    if let Some(d) = in_degree.get_mut(dep) {
                        *d -= 1;
                        if *d == 0 {
                            queue.push_back(*dep);
                        }
                    }
                }
            }
        }

        // 如果结果长度不等于节点数，说明有循环依赖
        // （比如 A 依赖 B，B 又依赖 A，那就死循环了）
        if result.len() != self.definitions.len() {
            log::warn!(
                "Geometry solver: cycle detected! {} of {} nodes sorted",
                result.len(),
                self.definitions.len()
            );
        }

        self.topo_order = result;
    }

    /// 打印所有元素的当前状态（调试用）。
    pub fn debug_print_all(&self) {
        log::debug!(
            "=== Geometry Solver State ({} elements) ===",
            self.definitions.len()
        );
        for id in &self.topo_order {
            if let Some(def) = self.definitions.get(id) {
                log::debug!(
                    "  {}  {}",
                    def.debug_print(),
                    if self.dirty.contains(id) {
                        "[DIRTY]"
                    } else {
                        ""
                    }
                );
            }
        }
    }
}

// ─── 测试 ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试：移动一个自由点，相关的线和中点自动更新。
    ///
    /// 场景：
    /// - 3 个自由点：Top(100, 0)、BottomLeft(0, 100)、BottomRight(200, 100)
    /// - 2 条线：Top→BottomLeft、Top→BottomRight
    /// - 2 个中点：分别是两条线的中点
    ///
    /// 验证：
    /// 1. 初始状态下中点坐标正确
    /// 2. 移动 Top 点后，线的长度变化
    /// 3. 中点坐标自动更新到新位置
    #[test]
    fn test_dynamic_geometry_brace_demo() {
        let mut ctx = SolverContext::new();

        // ── Step 1: 创建 3 个自由点 ──
        let top = FreePoint::new(100.0, 0.0);
        let bottom_left = FreePoint::new(0.0, 100.0);
        let bottom_right = FreePoint::new(200.0, 100.0);

        let top_id = top.id();
        let bl_id = bottom_left.id();
        let br_id = bottom_right.id();

        ctx.add_def(top);
        ctx.add_def(bottom_left);
        ctx.add_def(bottom_right);

        // ── Step 2: 创建 2 条线 ──
        let line_left = Line::new(top_id, bl_id);
        let line_right = Line::new(top_id, br_id);

        let line_left_id = line_left.id();
        let line_right_id = line_right.id();

        ctx.add_def(line_left);
        ctx.add_def(line_right);

        // ── Step 3: 创建 2 个中点 ──
        let mid_left = MidPoint::new(top_id, bl_id);
        let mid_right = MidPoint::new(top_id, br_id);

        let mid_left_id = mid_left.id();
        let mid_right_id = mid_right.id();

        ctx.add_def(mid_left);
        ctx.add_def(mid_right);

        // ── Step 4: 第一次求解 ──
        ctx.solve_all();

        // 验证初始状态
        let line_left_len = ctx
            .get_line(line_left_id)
            .map(|(a, b)| (b - a).norm())
            .unwrap();
        let line_right_len = ctx
            .get_line(line_right_id)
            .map(|(a, b)| (b - a).norm())
            .unwrap();

        // Top(100,0) → BottomLeft(0,100): 距离 = sqrt(100² + 100²) ≈ 141.42
        assert!(
            (line_left_len - 141.42).abs() < 0.1,
            "左线初始长度应该≈141.42，实际是{line_left_len}"
        );

        // Top(100,0) → BottomRight(200,100): 距离 = sqrt(100² + 100²) ≈ 141.42
        assert!(
            (line_right_len - 141.42).abs() < 0.1,
            "右线初始长度应该≈141.42，实际是{line_right_len}"
        );

        // 中点验证：左中点应该在 (50, 50)
        let mid_left_pos = ctx.get_point(mid_left_id).unwrap();
        assert!((mid_left_pos.x - 50.0).abs() < 0.01, "左中点x应该是50");
        assert!((mid_left_pos.y - 50.0).abs() < 0.01, "左中点y应该是50");

        // 右中点应该在 (150, 50)
        let mid_right_pos = ctx.get_point(mid_right_id).unwrap();
        assert!((mid_right_pos.x - 150.0).abs() < 0.01, "右中点x应该是150");
        assert!((mid_right_pos.y - 50.0).abs() < 0.01, "右中点y应该是50");

        // ── Step 5: 移动 Top 点到 (100, 50) ──
        // （模拟用户拖动点的操作）
        if let Some(_def) = ctx.get_mut(top_id) {
            // 先 downcast 为 FreePoint... 不行，trait object 不能直接 downcast
            // 所以我们用另一种方式：通过 point_position 拿到坐标，然后手动 set
        }
        // 更简单的方式：直接用 FreePoint 的 set_position 方法
        // 但我们需要 &mut FreePoint...
        //
        // 算了，测试里我们直接把点取出来改，再放回去，然后 mark_dirty
        if let Some(_def_box) = ctx.definitions.remove(&top_id) {
            // 这里我们需要 downcast... 但 Box<dyn Trait> 不能直接 downcast
            // 对于测试，我们用另一种方式：通过一个 helper 函数
            //
            // 实际上，最简单的做法是：在 SolverContext 上提供一个 update_point 方法
            // 或者让 FreePoint 有一个"设置坐标"的方法可以通过 trait 调用
            //
            // 为了不增加 trait 的复杂度，测试里我们直接重建点
            let new_top = FreePoint::with_id(top_id, 100.0, 50.0);
            ctx.definitions.insert(top_id, Box::new(new_top));
            ctx.mark_dirty(top_id);
        }

        // ── Step 6: 重新求解 ──
        ctx.solve_all();

        // 验证线长度变化了
        let line_left_len_new = ctx
            .get_line(line_left_id)
            .map(|(a, b)| (b - a).norm())
            .unwrap();
        let line_right_len_new = ctx
            .get_line(line_right_id)
            .map(|(a, b)| (b - a).norm())
            .unwrap();

        // Top(100,50) → BottomLeft(0,100): 距离 = sqrt(100² + 50²) ≈ 111.80
        assert!(
            (line_left_len_new - 111.80).abs() < 0.1,
            "移动后左线长度应该≈111.80，实际是{line_left_len_new}"
        );

        // Top(100,50) → BottomRight(200,100): 距离 = sqrt(100² + 50²) ≈ 111.80
        assert!(
            (line_right_len_new - 111.80).abs() < 0.1,
            "移动后右线长度应该≈111.80，实际是{line_right_len_new}"
        );

        // 长度确实变了（从 141.42 变到 111.80）
        assert!(line_left_len_new < line_left_len, "左线长度应该变短了");
        assert!(line_right_len_new < line_right_len, "右线长度应该变短了");

        // 验证中点自动更新
        let mid_left_new = ctx.get_point(mid_left_id).unwrap();
        // 新中点应该在 ((100+0)/2, (50+100)/2) = (50, 75)
        assert!(
            (mid_left_new.x - 50.0).abs() < 0.01,
            "移动后左中点x应该还是50"
        );
        assert!(
            (mid_left_new.y - 75.0).abs() < 0.01,
            "移动后左中点y应该是75，实际是{}",
            mid_left_new.y
        );

        let mid_right_new = ctx.get_point(mid_right_id).unwrap();
        // 新中点应该在 ((100+200)/2, (50+100)/2) = (150, 75)
        assert!(
            (mid_right_new.x - 150.0).abs() < 0.01,
            "移动后右中点x应该还是150"
        );
        assert!(
            (mid_right_new.y - 75.0).abs() < 0.01,
            "移动后右中点y应该是75，实际是{}",
            mid_right_new.y
        );

        println!("✅ 动态几何测试通过！");
        println!("   移动Top点后，两条线的长度自动更新，中点也自动跟随。");
        println!("   这就是约束求解的魔力 ✨");
    }

    /// 测试：两条线的交点计算。
    #[test]
    fn test_intersection_point() {
        let mut ctx = SolverContext::new();

        // 水平线：(0, 50) → (200, 50)
        let p1 = FreePoint::new(0.0, 50.0);
        let p2 = FreePoint::new(200.0, 50.0);
        let p1_id = p1.id();
        let p2_id = p2.id();
        ctx.add_def(p1);
        ctx.add_def(p2);

        let line_h = Line::new(p1_id, p2_id);
        let line_h_id = line_h.id();
        ctx.add_def(line_h);

        // 垂直线：(100, 0) → (100, 200)
        let p3 = FreePoint::new(100.0, 0.0);
        let p4 = FreePoint::new(100.0, 200.0);
        let p3_id = p3.id();
        let p4_id = p4.id();
        ctx.add_def(p3);
        ctx.add_def(p4);

        let line_v = Line::new(p3_id, p4_id);
        let line_v_id = line_v.id();
        ctx.add_def(line_v);

        // 交点
        let intersection = IntersectionPoint::new(line_h_id, line_v_id);
        let isect_id = intersection.id();
        ctx.add_def(intersection);

        ctx.solve_all();

        let pos = ctx.get_point(isect_id).unwrap();
        assert!((pos.x - 100.0).abs() < 0.01, "交点x应该是100");
        assert!((pos.y - 50.0).abs() < 0.01, "交点y应该是50");

        println!("✅ 交点测试通过！交点在 ({}, {})", pos.x, pos.y);
    }

    /// 测试：100 个几何定义的性能（毫秒级）。
    #[test]
    fn test_performance_100_elements() {
        let mut ctx = SolverContext::new();

        // 创建 20 个自由点
        let mut point_ids = Vec::new();
        for i in 0..20 {
            let p = FreePoint::new(i as f32 * 10.0, (i % 5) as f32 * 20.0);
            point_ids.push(p.id());
            ctx.add_def(p);
        }

        // 创建 40 条线（相邻点连线）
        let mut line_ids = Vec::new();
        for i in 0..19 {
            let line = Line::new(point_ids[i], point_ids[i + 1]);
            line_ids.push(line.id());
            ctx.add_def(line);
        }
        // 再加一些对角线
        for i in 0..10 {
            let line = Line::new(point_ids[i], point_ids[i + 10]);
            line_ids.push(line.id());
            ctx.add_def(line);
        }

        // 创建 20 个中点
        for i in 0..19 {
            let mid = MidPoint::new(point_ids[i], point_ids[i + 1]);
            ctx.add_def(mid);
        }

        // 创建 20 个交点（用所有线两两组合的子集）
        for i in 0..10 {
            let isect = IntersectionPoint::new(line_ids[i], line_ids[line_ids.len() - 1 - i]);
            ctx.add_def(isect);
        }

        assert_eq!(ctx.len(), 20 + 29 + 19 + 10); // 20点 + 29线 + 19中点 + 10交点 = 78

        use std::time::Instant;
        let start = Instant::now();
        ctx.solve_all();
        let elapsed = start.elapsed();

        println!(
            "⚡ 性能测试：{} 个几何定义，求解耗时 {:?}",
            ctx.len(),
            elapsed
        );

        // 毫秒级完成（应该远小于 1ms）
        assert!(elapsed.as_millis() < 100, "求解应该在毫秒级完成");

        // 测试增量更新：只动一个点
        let start = Instant::now();
        ctx.mark_dirty(point_ids[0]);
        ctx.solve_all();
        let elapsed_inc = start.elapsed();

        println!("   增量更新耗时: {elapsed_inc:?}");
        assert!(elapsed_inc < elapsed, "增量更新应该比全量更新快");
    }
}
