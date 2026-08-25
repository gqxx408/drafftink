//! 物理图元库 —— 纯 egui 绘制，零 WebView 依赖。
//!
//! 包含 5 种基础物理元件：
//! - Resistor  (电阻)   — 矩形 + 内部锯齿线
//! - Bulb      (灯泡)   — 圆圈 + 底部三角底座
//! - Battery   (电源)   — 一长一短两条竖线
//! - Lens      (透镜)   — 两侧弧形（凸透镜/凹透镜）
//! - Mirror    (平面镜) — 直线 + 背面斜线阴影
//!
//! 所有图元统一用 egui::Painter 绘制，内存占用极低。

use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2};
use uuid::Uuid;

// ─── 物理图元类型 ──────────────────────────────────────────────────────────

/// 物理图元类型枚举。
///
/// 每种图元都有一个位置（左上角）和尺寸，
/// 便于统一处理拖拽、选中、碰撞检测等交互逻辑。
#[derive(Clone, Debug)]
pub enum PhysicsElement {
    /// 电阻：矩形 + 内部锯齿状线条
    Resistor(ResistorData),
    /// 灯泡：圆形 + 底部三角底座
    Bulb(BulbData),
    /// 电源：一长一短两条竖线（长正短负）
    Battery(BatteryData),
    /// 透镜：凸透镜或凹透镜
    Lens(LensData),
    /// 平面镜：直线 + 背面斜线阴影
    Mirror(MirrorData),
}

/// 所有图元共享的基础属性。
#[derive(Clone, Debug)]
pub struct ElementBase {
    pub id: Uuid,
    /// 图元左上角位置（画布坐标）
    pub position: Pos2,
    /// 图元尺寸
    pub size: Vec2,
    /// 是否被选中
    pub selected: bool,
}

impl ElementBase {
    pub fn new(pos: Pos2, size: Vec2) -> Self {
        Self {
            id: Uuid::new_v4(),
            position: pos,
            size,
            selected: false,
        }
    }

    /// 图元的包围盒矩形
    pub fn rect(&self) -> Rect {
        Rect::from_min_size(self.position, self.size)
    }

    /// 判断一个点是否在图元范围内
    pub fn contains(&self, pos: Pos2) -> bool {
        self.rect().contains(pos)
    }
}

// ─── 电阻 ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct ResistorData {
    pub base: ElementBase,
}

impl ResistorData {
    pub fn new(pos: Pos2) -> Self {
        Self {
            base: ElementBase::new(pos, Vec2::new(100.0, 40.0)),
        }
    }
}

// ─── 灯泡 ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct BulbData {
    pub base: ElementBase,
}

impl BulbData {
    pub fn new(pos: Pos2) -> Self {
        Self {
            base: ElementBase::new(pos, Vec2::new(60.0, 80.0)),
        }
    }
}

// ─── 电源 ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct BatteryData {
    pub base: ElementBase,
}

impl BatteryData {
    pub fn new(pos: Pos2) -> Self {
        Self {
            base: ElementBase::new(pos, Vec2::new(60.0, 80.0)),
        }
    }
}

// ─── 透镜 ──────────────────────────────────────────────────────────────────

/// 透镜类型
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
pub enum LensType {
    /// 凸透镜（中间厚边缘薄，会聚光线）
    Convex,
    /// 凹透镜（中间薄边缘厚，发散光线）
    Concave,
}

#[derive(Clone, Debug)]
pub struct LensData {
    pub base: ElementBase,
    pub lens_type: LensType,
}

impl LensData {
    pub fn new(pos: Pos2) -> Self {
        Self {
            base: ElementBase::new(pos, Vec2::new(40.0, 100.0)),
            lens_type: LensType::Convex,
        }
    }
}

// ─── 平面镜 ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct MirrorData {
    pub base: ElementBase,
}

impl MirrorData {
    pub fn new(pos: Pos2) -> Self {
        Self {
            base: ElementBase::new(pos, Vec2::new(100.0, 30.0)),
        }
    }
}

// ─── 统一访问 ──────────────────────────────────────────────────────────────

impl PhysicsElement {
    pub fn base(&self) -> &ElementBase {
        match self {
            PhysicsElement::Resistor(d) => &d.base,
            PhysicsElement::Bulb(d) => &d.base,
            PhysicsElement::Battery(d) => &d.base,
            PhysicsElement::Lens(d) => &d.base,
            PhysicsElement::Mirror(d) => &d.base,
        }
    }

    pub fn base_mut(&mut self) -> &mut ElementBase {
        match self {
            PhysicsElement::Resistor(d) => &mut d.base,
            PhysicsElement::Bulb(d) => &mut d.base,
            PhysicsElement::Battery(d) => &mut d.base,
            PhysicsElement::Lens(d) => &mut d.base,
            PhysicsElement::Mirror(d) => &mut d.base,
        }
    }

    pub fn id(&self) -> Uuid {
        self.base().id
    }

    #[allow(dead_code)]
    pub fn rect(&self) -> Rect {
        self.base().rect()
    }

    pub fn contains(&self, pos: Pos2) -> bool {
        self.base().contains(pos)
    }

    pub fn set_selected(&mut self, selected: bool) {
        self.base_mut().selected = selected;
    }

    /// 图元的中文名（用于工具栏按钮）
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            PhysicsElement::Resistor(_) => "电阻",
            PhysicsElement::Bulb(_) => "灯泡",
            PhysicsElement::Battery(_) => "电源",
            PhysicsElement::Lens(_) => "透镜",
            PhysicsElement::Mirror(_) => "平面镜",
        }
    }
}

// ─── 绘制常量 ──────────────────────────────────────────────────────────────

const STROKE_COLOR: Color32 = Color32::from_rgb(30, 30, 30);
const STROKE_WIDTH: f32 = 2.0;
const SELECTED_COLOR: Color32 = Color32::from_rgb(58, 134, 255);
#[allow(dead_code)]
const FILL_COLOR: Color32 = Color32::TRANSPARENT;

// ─── 绘制函数 ──────────────────────────────────────────────────────────────

/// 绘制一个物理图元。
pub fn draw_element(painter: &Painter, elem: &PhysicsElement) {
    let base = elem.base();
    let stroke = if base.selected {
        Stroke::new(STROKE_WIDTH + 1.0, SELECTED_COLOR)
    } else {
        Stroke::new(STROKE_WIDTH, STROKE_COLOR)
    };

    match elem {
        PhysicsElement::Resistor(d) => draw_resistor(painter, &d.base, stroke),
        PhysicsElement::Bulb(d) => draw_bulb(painter, &d.base, stroke),
        PhysicsElement::Battery(d) => draw_battery(painter, &d.base, stroke),
        PhysicsElement::Lens(d) => draw_lens(painter, &d.base, &d.lens_type, stroke),
        PhysicsElement::Mirror(d) => draw_mirror(painter, &d.base, stroke),
    }

    // 选中时绘制虚线包围盒
    if base.selected {
        let rect = base.rect();
        painter.rect_stroke(
            rect.expand(4.0),
            2.0,
            Stroke {
                width: 1.0,
                color: SELECTED_COLOR,
            },
        );
    }
}

/// 电阻：矩形 + 内部锯齿状线条
///
/// ```text
///   ┌──────────────────────────┐
///   │  /\  /\  /\  /\  /\  /\  │
///   │ /  \/  \/  \/  \/  \/  \ │
///   └──────────────────────────┘
/// ```
fn draw_resistor(painter: &Painter, base: &ElementBase, stroke: Stroke) {
    let rect = base.rect();

    // 外框矩形
    painter.rect_stroke(rect, 0.0, stroke);

    // 内部锯齿线
    let teeth = 6; // 锯齿数量
    let tooth_width = rect.width() / teeth as f32;
    let tooth_height = rect.height() * 0.4;
    let mid_y = rect.center().y;

    let mut points = Vec::with_capacity(teeth * 2 + 1);
    for i in 0..=teeth {
        let x = rect.left() + i as f32 * tooth_width;
        if i % 2 == 0 {
            // 偶数齿：在下半
            points.push(Pos2::new(x, mid_y - tooth_height));
            points.push(Pos2::new(x + tooth_width * 0.5, mid_y + tooth_height));
        } else {
            // 奇数齿：在上半
            points.push(Pos2::new(x + tooth_width * 0.5, mid_y + tooth_height));
            points.push(Pos2::new(x + tooth_width, mid_y - tooth_height));
        }
    }

    // 用线段连接锯齿（去掉最后一个多余的点）
    for i in 0..points.len().saturating_sub(1) {
        painter.line_segment([points[i], points[i + 1]], stroke);
    }
}

/// 灯泡：圆圈 + 底部三角底座
///
/// ```text
///        ╭───╮
///       /     \
///      │   ●   │  ← 圆圈（灯泡）
///       \     /
///        ╰───╯
///         / \
///        /   \    ← 三角底座
///       /─────\
/// ```
fn draw_bulb(painter: &Painter, base: &ElementBase, stroke: Stroke) {
    let rect = base.rect();
    let center = Pos2::new(rect.center().x, rect.top() + rect.width() * 0.6);
    let radius = rect.width() * 0.4;

    // 灯泡圆圈
    painter.circle_stroke(center, radius, stroke);

    // 底部三角底座
    let base_top_y = center.y + radius * 0.6;
    let base_bottom_y = rect.bottom();
    let base_half_w = radius * 0.4;

    let tri_top = Pos2::new(center.x, base_top_y);
    let tri_bl = Pos2::new(center.x - base_half_w, base_bottom_y);
    let tri_br = Pos2::new(center.x + base_half_w, base_bottom_y);

    painter.line_segment([tri_top, tri_bl], stroke);
    painter.line_segment([tri_top, tri_br], stroke);
    painter.line_segment([tri_bl, tri_br], stroke);

    // 灯泡内部的小十字（灯丝示意）
    let cross_size = radius * 0.3;
    painter.line_segment(
        [
            Pos2::new(center.x - cross_size, center.y),
            Pos2::new(center.x + cross_size, center.y),
        ],
        stroke,
    );
    painter.line_segment(
        [
            Pos2::new(center.x, center.y - cross_size),
            Pos2::new(center.x, center.y + cross_size),
        ],
        stroke,
    );
}

/// 电源：一长一短两条竖线（长=正极，短=负极）
///
/// ```text
///       │       │
///       │       │
///       │       │
///     长线     短线
///     (正极)   (负极)
/// ```
fn draw_battery(painter: &Painter, base: &ElementBase, stroke: Stroke) {
    let rect = base.rect();
    let center_x = rect.center().x;
    let gap = rect.width() * 0.35; // 两条线之间的间距

    // 长线（正极，左边）
    let long_x = center_x - gap;
    let long_top = rect.top() + rect.height() * 0.1;
    let long_bottom = rect.bottom() - rect.height() * 0.1;
    painter.line_segment(
        [Pos2::new(long_x, long_top), Pos2::new(long_x, long_bottom)],
        stroke,
    );

    // 短线（负极，右边）— 长度约为长线的一半
    let short_x = center_x + gap;
    let short_len = (long_bottom - long_top) * 0.45;
    let short_top = rect.center().y - short_len * 0.5;
    let short_bottom = rect.center().y + short_len * 0.5;
    painter.line_segment(
        [
            Pos2::new(short_x, short_top),
            Pos2::new(short_x, short_bottom),
        ],
        stroke,
    );

    // 顶部和底部的水平连接线（表示电池两端的接线）
    let wire_extend = rect.width() * 0.3;
    // 顶部接线
    painter.line_segment(
        [
            Pos2::new(long_x - wire_extend, long_top),
            Pos2::new(long_x, long_top),
        ],
        stroke,
    );
    painter.line_segment(
        [
            Pos2::new(short_x, short_top),
            Pos2::new(short_x + wire_extend, short_top),
        ],
        stroke,
    );
    // 底部接线
    painter.line_segment(
        [
            Pos2::new(long_x - wire_extend, long_bottom),
            Pos2::new(long_x, long_bottom),
        ],
        stroke,
    );
    painter.line_segment(
        [
            Pos2::new(short_x, short_bottom),
            Pos2::new(short_x + wire_extend, short_bottom),
        ],
        stroke,
    );
}

/// 透镜：凸透镜或凹透镜
///
/// 凸透镜（Convex）：中间厚，边缘薄 —— 左右两侧都向外凸
/// ```text
///     ╭─╮
///    /   \
///   │     │
///    \   /
///     ╰─╯
/// ```
///
/// 凹透镜（Concave）：中间薄，边缘厚 —— 左右两侧都向内凹
/// ```text
///     ╭─╮
///   ╱     ╲
///  │       │
///   ╲     ╱
///     ╰─╯
/// ```
fn draw_lens(painter: &Painter, base: &ElementBase, lens_type: &LensType, stroke: Stroke) {
    let rect = base.rect();
    let left = rect.left();
    let right = rect.right();
    let top = rect.top();
    let bottom = rect.bottom();
    let mid_x = rect.center().x;
    let mid_y = rect.center().y;

    // 透镜厚度参数
    let edge_thickness = rect.width() * 0.25; // 边缘厚度
    let center_thickness = match lens_type {
        LensType::Convex => rect.width() * 0.8,  // 凸透镜：中间厚
        LensType::Concave => rect.width() * 0.1, // 凹透镜：中间薄
    };

    // 用二次贝塞尔曲线画两侧弧线
    // 左侧弧
    let left_top = Pos2::new(mid_x - center_thickness / 2.0, top);
    let left_bottom = Pos2::new(mid_x - center_thickness / 2.0, bottom);
    let left_cp = Pos2::new(left + edge_thickness / 2.0, mid_y);

    // 右侧弧
    let right_top = Pos2::new(mid_x + center_thickness / 2.0, top);
    let right_bottom = Pos2::new(mid_x + center_thickness / 2.0, bottom);
    let right_cp = Pos2::new(right - edge_thickness / 2.0, mid_y);

    // 画左侧弧（用二次贝塞尔）
    // egui 没有直接的 quadratic bezier，我们用分段直线模拟
    draw_quadratic_bezier(painter, left_top, left_cp, left_bottom, stroke);

    // 画右侧弧
    draw_quadratic_bezier(painter, right_top, right_cp, right_bottom, stroke);

    // 顶部和底部的水平边
    painter.line_segment([left_top, right_top], stroke);
    painter.line_segment([left_bottom, right_bottom], stroke);

    // 主光轴（水平虚线穿过中心）
    let axis_stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(100, 100, 100, 180));
    let dash_len = 6.0;
    let gap_len = 4.0;
    let mut x = left - rect.width() * 0.3;
    let end_x = right + rect.width() * 0.3;
    while x < end_x {
        let seg_end = (x + dash_len).min(end_x);
        painter.line_segment(
            [Pos2::new(x, mid_y), Pos2::new(seg_end, mid_y)],
            axis_stroke,
        );
        x = seg_end + gap_len;
    }
}

/// 平面镜：直线 + 背面斜线阴影
///
/// ```text
///   反射面（光滑直线）
///   │
///   │//////////  ← 背面斜线阴影
///   │
/// ```
fn draw_mirror(painter: &Painter, base: &ElementBase, stroke: Stroke) {
    let rect = base.rect();
    let left = rect.left();
    let _right = rect.right();
    let top = rect.top();
    let bottom = rect.bottom();

    // 反射面（左侧的垂直直线）
    painter.line_segment([Pos2::new(left, top), Pos2::new(left, bottom)], stroke);

    // 背面斜线阴影（从左向右下倾斜的短线条）
    let hatch_spacing = 8.0;
    let hatch_length = 10.0;
    let hatch_stroke = Stroke::new(1.5, STROKE_COLOR);

    let mut y = top;
    while y <= bottom {
        let x_start = left + 4.0;
        let x_end = x_start + hatch_length;
        let y_end = y + hatch_length * 0.6; // 60 度斜线
        painter.line_segment(
            [Pos2::new(x_start, y), Pos2::new(x_end, y_end)],
            hatch_stroke,
        );
        y += hatch_spacing;
    }

    // 上下端的小横线（表示镜子的边界）
    let cap_w = 6.0;
    painter.line_segment([Pos2::new(left, top), Pos2::new(left + cap_w, top)], stroke);
    painter.line_segment(
        [Pos2::new(left, bottom), Pos2::new(left + cap_w, bottom)],
        stroke,
    );
}

// ─── 辅助函数：二次贝塞尔曲线 ──────────────────────────────────────────────

/// 用分段直线近似二次贝塞尔曲线。
///
/// egui 原生只提供了 CubicBezierShape，没有 QuadraticBezierShape，
/// 我们把二次贝塞尔通过采样转成多段直线，效果一样顺滑。
fn draw_quadratic_bezier(painter: &Painter, p0: Pos2, p1: Pos2, p2: Pos2, stroke: Stroke) {
    let segments = 20; // 采样数，越高越平滑
    let mut prev = p0;

    for i in 1..=segments {
        let t = i as f32 / segments as f32;
        let u = 1.0 - t;
        // 二次贝塞尔公式: B(t) = (1-t)²P0 + 2(1-t)tP1 + t²P2
        let x = u * u * p0.x + 2.0 * u * t * p1.x + t * t * p2.x;
        let y = u * u * p0.y + 2.0 * u * t * p1.y + t * t * p2.y;
        let curr = Pos2::new(x, y);
        painter.line_segment([prev, curr], stroke);
        prev = curr;
    }
}
