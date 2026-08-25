//! 虚拟教具覆盖层（圆规 / 三角尺 / 量角器）。
//!
//! 设计哲学：教具是**临时的「向导」**，不是一次性绘制工具。老师在顶栏选择教具后，
//! 画布上出现一个可拖拽的智能对象，交互结束后「提交」为标准的
//! [`crate::app::ShapeInstance`]（纳入 `shape_instances` + Undo 栈），教具对象随即销毁。
//! 因此教具状态不序列化、不进 ENBX，提交后的元素才可序列化 / Undo / 与视频图片共存。
//!
//! 角度统一约定（与 `shape_renderer` 一致）：**0° = 正右，逆时针为正，屏幕 y 向下**。

use egui::{Align2, Color32, FontId, Painter, Pos2, Stroke, Vec2};

use crate::function_parser::Expr;

/// 圆规模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompassMode {
    Circle,
    Arc,
    Sector,
}

/// 圆规：转轴（圆心）+ 铅笔脚（半径端点）。
#[derive(Debug, Clone)]
pub struct CompassTool {
    /// 转轴（圆心）位置。
    pub pivot: Pos2,
    /// 铅笔脚位置。
    pub pencil: Pos2,
    /// 提交模式。
    pub mode: CompassMode,
    /// 画弧 / 扇形时的起始角（度）。
    pub arc_start_deg: f32,
    /// 画弧 / 扇形时的终止角（度）。
    pub arc_end_deg: f32,
    /// 交互阶段：0 = 未确定圆心；1 = 圆心已定、拖动铅笔脚确定半径。
    pub stage: u8,
}

impl CompassTool {
    /// 实时半径 = 转轴到铅笔脚距离。
    pub fn radius(&self) -> f32 {
        self.pivot.distance(self.pencil)
    }
}

/// 三角尺规格。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetSquareKind {
    Triangle30_60_90,
    Triangle45_45_90,
}

/// 三角尺：直角顶点 + 旋转角 + 直角边长。
#[derive(Debug, Clone)]
pub struct SetSquareTool {
    pub kind: SetSquareKind,
    /// 直角顶点位置。
    pub origin: Pos2,
    /// 当前旋转角度（度）。
    pub rotation_deg: f32,
    /// 直角边长（屏幕 px）。
    pub size: f32,
    /// 是否正在拖拽移动三角尺（重心拖拽或整体拖拽）。
    pub moving: bool,
    /// 是否正在沿某条边画线。
    pub drawing: bool,
    /// 是否正在拖拽任一顶点做旋转（绕直角顶点 origin 鼠标追踪）。
    pub rotating: bool,
    /// 沿线画线：起点（吸附到边上的最近点）。
    pub line_start: Pos2,
    /// 沿线画线：当前点（跟随鼠标，吸附到所选边方向）。
    pub line_current: Pos2,
    /// 沿线画线：所选边序号（0=底边、1=斜边、2=另一条直角边），用于方向吸附。
    pub line_edge: Option<usize>,
}

/// 量角器模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtractorMode {
    /// 测量：鼠标移动实时显示角度。
    Measure,
    /// 画角：点两次确定两条边。
    DrawAngle,
    /// 画弧：拖拽扫过角度。
    DrawArc,
}

/// 量角器：半圆 0°–180°。
#[derive(Debug, Clone)]
pub struct ProtractorTool {
    /// 半圆圆心（角度顶点）。
    pub center: Pos2,
    /// 量角器半径（屏幕 px）。
    pub radius: f32,
    /// 整体旋转（度）。量角器现在仅支持平移、不支持旋转，此字段仅为统一接口保留（值恒 0）。
    #[allow(dead_code)]
    pub rotation_deg: f32,
    /// 当前鼠标对应的角度（量角器读数 0°–180°）。
    pub cursor_angle_deg: f32,
    pub mode: ProtractorMode,
    /// 画角模式：第一条边的角度（量角器读数）；`None` 表示尚未点击第一条边。
    pub first_angle_deg: Option<f32>,
    /// 是否正在拖拽移动量角器（按住外圈空白处整体平移）。
    pub dragging: bool,
    /// 拖拽移动：上一帧鼠标位置（用于计算每帧 delta）。
    pub last_mouse: Pos2,
}

/// 直尺拖拽的端点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhichEnd {
    Start,
    End,
}

/// 直尺：两端点 + 拖拽状态。
#[derive(Debug, Clone)]
pub struct RulerTool {
    /// 左端点（标尺 0 刻度起点）。
    pub start: Pos2,
    /// 右端点。
    pub end: Pos2,
    /// 正在拖拽哪一端（`None` 表示未拖端点）。
    pub dragging_end: Option<WhichEnd>,
    /// 是否正在拖拽整体平移。
    pub dragging_body: bool,
    /// 整体平移时的上一帧鼠标位置（用于计算每帧 delta）。
    pub last_mouse: Pos2,
}

/// 正多边形：中心 + 半径 + 边数 + 预览旋转角。
#[derive(Debug, Clone)]
pub struct PolygonTool {
    /// 多边形外接圆圆心。
    pub center: Pos2,
    /// 外接圆半径（px）。`0.0` 表示尚未确定中心（等待第一次点击）。
    pub radius: f32,
    /// 边数（3–12）。
    pub sides: u8,
    /// 预览旋转角（度），滚轮或 Q/E 调节，让三角形底边水平等。
    pub preview_angle: f32,
}

/// 函数绘图：坐标系 + 表达式曲线（`y = f(x)`）预览。
///
/// 交互：激活后在画布中心放置坐标系，表达式框输入 `y = 2x+1` / `sin(x)` 等，
/// 实时预览曲线；Enter / 双击提交为文档层 `SvgShape`（polyline path）。
#[derive(Debug, Clone)]
pub struct FunctionPlotTool {
    /// 坐标系中心（屏幕坐标）。
    pub center: Pos2,
    /// 每单位长度像素（如 40px/单位）。
    pub scale: f32,
    /// 表达式文本（来自输入框）。
    pub expr_str: String,
    /// 实时解析结果（`Ok` 时预览曲线）。
    pub parsed: Option<Expr>,
    /// 解析错误信息（`Some` 时显示红色提示，不 panic）。
    pub error: Option<String>,
}

/// 数轴：点击定起点 → 拖拽定终点 → Shift 吸附水平/垂直 → 松开提交。
///
/// 与直尺的端点拖拽不同，数轴是**一次性拖拽**（按下定起点、拖动定终点、松开即提交），
/// 交互更接近「画线段」。
#[derive(Debug, Clone)]
pub struct NumberLineTool {
    /// 起点（按下时固定）。
    pub start: Pos2,
    /// 终点（拖拽中跟随鼠标，Shift 时吸附水平/垂直）。
    pub end: Pos2,
    /// 是否正在拖拽确定终点。
    pub dragging: bool,
    /// 每刻度像素间距（px）。
    pub step: f32,
    /// 每 N 个刻度标一个数字（≥1）。
    pub label_interval: i32,
}

/// 倒计时器：授课模式计时工具（纯 UI 覆盖层，不序列化）。
///
/// 交互：激活后画布出现半透明圆角矩形（宽 200×高 80），未设置时间时弹输入框
/// （M:SS 或纯秒）→ 点击数字开始/暂停 → 拖拽移动 → 到 0 红白闪烁 → ✕ 或 Esc 关闭。
#[derive(Debug, Clone)]
pub struct CountdownTool {
    /// 设定的总秒数（解析输入后固定）。
    pub total_seconds: u32,
    /// 剩余秒数（倒计时递减）。
    pub remaining_seconds: u32,
    pub is_running: bool,
    /// 到时后置 true（触发红白闪烁）。
    pub is_finished: bool,
    /// 计时器左上角位置（画布屏幕坐标，可拖拽）。
    pub position: Pos2,
    /// 输入框文本（如 `"05:30"` / `"330"`）。
    pub input_text: String,
    /// 上次 tick 时刻（驱动每秒递减）。
    pub last_tick: Option<std::time::Instant>,
    /// 按下数字区时记录起点（区分点击 vs 拖拽）。
    pub pending_press: Option<Pos2>,
    /// 是否正在拖拽移动计时器。
    pub dragging: bool,
    /// 拖拽中上一帧鼠标位置。
    pub last_mouse: Pos2,
}

impl Default for CountdownTool {
    fn default() -> Self {
        Self {
            total_seconds: 0,
            remaining_seconds: 0,
            is_running: false,
            is_finished: false,
            position: Pos2::ZERO,
            input_text: String::new(),
            last_tick: None,
            pending_press: None,
            dragging: false,
            last_mouse: Pos2::ZERO,
        }
    }
}

/// `秒` → `MM:SS` 文本（纯函数，可单测）。
pub fn format_mmss(secs: u32) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

impl CountdownTool {
    /// 解析输入文本（M:SS / MM:SS / 纯秒），成功则设置 total 与 remaining。
    /// 失败返回 `Err(描述)`（不 panic，输入框内红字提示）。
    pub fn parse_input(&mut self) -> Result<(), String> {
        let s = self.input_text.trim();
        let secs = if let Ok(v) = s.parse::<u32>() {
            v
        } else if let Some((m, sec)) = s.split_once(':') {
            let m = m.trim().parse::<u32>().map_err(|_| format!("无效分钟 '{m}'"))?;
            let sec = sec.trim().parse::<u32>().map_err(|_| format!("无效秒 '{sec}'"))?;
            if sec >= 60 {
                return Err(format!("秒数不能 ≥ 60（'{sec}'）"));
            }
            m.saturating_mul(60).saturating_add(sec)
        } else {
            return Err(format!("无法解析 '{s}'（用 M:SS 或纯秒）"));
        };
        if secs == 0 {
            return Err("时间不能为 0".to_string());
        }
        self.total_seconds = secs.min(5999); // 上限 99:59
        self.remaining_seconds = self.total_seconds;
        self.is_running = true;
        self.is_finished = false;
        self.last_tick = None;
        Ok(())
    }

    /// 每秒递减一次（由 `update_active_tool` 按 `Instant` 驱动）。
    pub fn tick(&mut self) {
        if self.is_running && !self.is_finished && self.remaining_seconds > 0 {
            self.remaining_seconds -= 1;
            if self.remaining_seconds == 0 {
                self.is_finished = true;
                self.is_running = false;
            }
        }
    }

    /// 数字颜色：到 0 后按 `time` 每 500ms 红/白闪烁；最后 10 秒红色；否则白色。
    pub fn flash_color(&self, time: f64) -> Color32 {
        if self.is_finished {
            if (time * 2.0) as i32 % 2 == 0 {
                Color32::RED
            } else {
                Color32::WHITE
            }
        } else if self.remaining_seconds <= 10 && self.remaining_seconds > 0 {
            Color32::RED
        } else {
            Color32::WHITE
        }
    }

    /// 计时器矩形（200×80，position 为左上角）。
    pub fn rect(&self) -> egui::Rect {
        egui::Rect::from_min_size(self.position, egui::vec2(200.0, 80.0))
    }
}

/// 倒计时器画布绘制：半透明黑圆角背景 + 白边框 + 大号数字 + 右上角 ✕。
///
/// `time` 为 `ctx.input(|i| i.time)`，驱动到 0 后的红白闪烁。
pub fn draw_countdown(painter: &Painter, tool: &CountdownTool, time: f64) {
    let rect = tool.rect();
    painter.rect_filled(rect, 8.0, Color32::from_black_alpha(180));
    painter.rect_stroke(rect, 8.0, Stroke::new(2.0, Color32::WHITE));
    let text = format_mmss(tool.remaining_seconds);
    let color = tool.flash_color(time);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::monospace(36.0),
        color,
    );
    // 右上角 ✕ 关闭按钮。
    let close_rect = egui::Rect::from_min_size(
        rect.right_top() + Vec2::new(-20.0, 4.0),
        Vec2::new(16.0, 16.0),
    );
    painter.text(
        close_rect.center(),
        Align2::CENTER_CENTER,
        "✕",
        FontId::proportional(14.0),
        Color32::GRAY,
    );
}

/// 当前激活的教具。
#[derive(Debug, Clone, Default)]
pub enum ActiveTool {
    #[default]
    None,
    Compass(CompassTool),
    SetSquare(SetSquareTool),
    Protractor(ProtractorTool),
    Ruler(RulerTool),
    Polygon(PolygonTool),
    FunctionPlot(FunctionPlotTool),
    NumberLine(NumberLineTool),
    Countdown(CountdownTool),
}

// ─────────────────────────────────────────────────────────────────────────────
// 纯几何 helper（可单测）
// ─────────────────────────────────────────────────────────────────────────────

/// 常用吸附角（度）。
pub const SNAP_ANGLES: [f32; 4] = [30.0, 45.0, 60.0, 90.0];

/// 把 `raw`（度）吸附到最近的常用角；仅当偏差 < `tolerance_deg`（默认 3°）时生效，
/// 否则返回原值。返回 `(吸附后的角, 是否吸附)`。
pub fn snap_angle(raw: f32, tolerance_deg: f32) -> (f32, bool) {
    let mut best = raw;
    let mut best_diff = tolerance_deg;
    let mut snapped = false;
    for &a in &SNAP_ANGLES {
        let diff = (a - raw).abs();
        if diff < best_diff {
            best = a;
            best_diff = diff;
            snapped = true;
        }
    }
    (if snapped { best } else { raw }, snapped)
}

/// 把角度吸附到 15° 网格（Shift + 拖拽旋转时用），返回吸附后的角度（度）。
pub fn snap_angle_grid15(raw: f32) -> f32 {
    let grid = 15.0_f32;
    (raw / grid).round() * grid
}

/// 反推：`center → p` 的方向角（统一约定，0°=正右、逆时针为正）。
pub fn angle_of(center: Pos2, p: Pos2) -> f32 {
    let d = p - center;
    (-d.y).atan2(d.x).to_degrees()
}

/// 点到线段的最短距离（返回距离 + 线段上的最近点）。
pub fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> (f32, Pos2) {
    let ab = b - a;
    let len2 = ab.length_sq();
    if len2 < 1e-6 {
        return (p.distance(a), a);
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    let nearest = a + ab * t;
    (p.distance(nearest), nearest)
}

/// 点到无限直线的最近点（画线时把鼠标吸附到所选边方向）。
pub fn closest_point_on_line(p: Pos2, a: Pos2, b: Pos2) -> Pos2 {
    let ab = b - a;
    let len2 = ab.length_sq();
    if len2 < 1e-6 {
        return a;
    }
    let t = (p - a).dot(ab) / len2;
    a + ab * t
}

/// 由「量角器读数」（0°=左、180°=右）转换为统一角度（0°=正右）。
pub fn protractor_to_unified(deg: f32) -> f32 {
    180.0 - deg
}

/// 由统一角度（0°=正右）转换为「量角器读数」（0°=左、180°=右），并夹到 0–180。
pub fn unified_to_protractor(deg: f32) -> f32 {
    let mut d = 180.0 - deg;
    while d < 0.0 {
        d += 360.0;
    }
    while d > 360.0 {
        d -= 360.0;
    }
    d.clamp(0.0, 180.0)
}

/// 旋转向量（弧度，逆时针）。egui 0.29 的 `Vec2` 无 `rotated` 方法，故手写。
pub fn rotate_vec(v: Vec2, angle_rad: f32) -> Vec2 {
    let (s, c) = angle_rad.sin_cos();
    Vec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

/// 三角尺三个顶点（直角顶点 + 两锐角顶点）。
pub fn set_square_points(t: &SetSquareTool) -> [Pos2; 3] {
    let rot = t.rotation_deg.to_radians();
    match t.kind {
        SetSquareKind::Triangle30_60_90 => [
            t.origin,
            t.origin + rotate_vec(Vec2::new(t.size, 0.0), rot),
            t.origin + rotate_vec(Vec2::new(0.0, t.size * 1.732_050_8), rot),
        ],
        SetSquareKind::Triangle45_45_90 => [
            t.origin,
            t.origin + rotate_vec(Vec2::new(t.size, 0.0), rot),
            t.origin + rotate_vec(Vec2::new(t.size, t.size), rot),
        ],
    }
}

/// 三角尺重心（几何中心），作为旋转手柄的锚点（三顶点坐标平均）。
pub fn set_square_centroid(t: &SetSquareTool) -> Pos2 {
    let pts = set_square_points(t);
    Pos2::new(
        (pts[0].x + pts[1].x + pts[2].x) / 3.0,
        (pts[0].y + pts[1].y + pts[2].y) / 3.0,
    )
}

/// 三角尺三条边（线段对）。边 0 = 底边（直角顶点→顶点1），边 1 = 斜边（顶点1→顶点2），
/// 边 2 = 另一条直角边（顶点2→直角顶点）。
pub fn set_square_edges(t: &SetSquareTool) -> [(Pos2, Pos2); 3] {
    let pts = set_square_points(t);
    [(pts[0], pts[1]), (pts[1], pts[2]), (pts[2], pts[0])]
}

/// 在三角尺三条边中找离 `p` 最近的一条；若最近距离 ≥ `max_dist` 返回 `None`。
/// 返回 `(边序号, 吸附到该边上的最近点)`。
pub fn find_nearest_edge(p: Pos2, t: &SetSquareTool, max_dist: f32) -> Option<(usize, Pos2)> {
    let mut best = (0usize, f32::INFINITY, p);
    for (i, (a, b)) in set_square_edges(t).iter().enumerate() {
        let (d, snap) = dist_to_segment(p, *a, *b);
        if d < best.1 {
            best = (i, d, snap);
        }
    }
    (best.1 < max_dist).then_some((best.0, best.2))
}

/// 沿线画线提交判定：起止点距离超过 `min_len` 时返回线段两端点，否则返回 `None`（防误触）。
pub fn line_draw_result(start: Pos2, current: Pos2, min_len: f32) -> Option<(Pos2, Pos2)> {
    (start.distance(current) > min_len).then_some((start, current))
}

// ─────────────────────────────────────────────────────────────────────────────
// 教具统一灰色视觉常量（三角尺 / 直尺共用）
// ─────────────────────────────────────────────────────────────────────────────

/// 教具主体描边与 cm 主刻度（深灰，2px 边框 / 1.5px 主刻度共用）。
pub const SETSQUARE_GRAY: Color32 = Color32::from_gray(50);
/// 教具半透明极淡灰填充（三角尺面 / 直尺尺身）。
pub const SETSQUARE_FILL: Color32 = Color32::from_rgba_premultiplied(170, 170, 170, 46);
/// 次刻度（mm 短刻度 / 半 cm 中刻度）的灰色。
pub const SETSQUARE_TICK_GRAY: Color32 = Color32::from_gray(115);
/// 刻度数字 / 标注文本的灰色。
pub const SETSQUARE_TEXT_GRAY: Color32 = Color32::from_gray(78);

/// 直尺每厘米对应的屏幕像素（37.8 px/cm → 1mm ≈ 3.78px）。
pub const PIXELS_PER_CM: f32 = 37.8;

/// 直尺方向单位向量（start → end）；零长度时返回零向量。
pub fn ruler_dir(t: &RulerTool) -> Vec2 {
    (t.end - t.start).normalized()
}

/// 把方向向量吸附到最近的 45° 网格（0°=水平、90°=竖直、45° 对角），保持长度不变。
/// Shift + 拖拽端点时用。
pub fn snap_dir_grid45(v: Vec2) -> Vec2 {
    let len = v.length();
    if len <= 0.0 {
        return Vec2::ZERO;
    }
    // 统一角度约定：0°=正右、逆时针为正（y 向下）。
    let ang = (-v.y).atan2(v.x);
    let snapped = (ang / std::f32::consts::FRAC_PI_4).round() * std::f32::consts::FRAC_PI_4;
    Vec2::new(snapped.cos(), -snapped.sin()) * len
}

/// 直尺第 `mm_index` 个毫米刻度的位置（0 = start 端点，沿 `dir` 方向）。
/// `pixels_per_cm` 决定毫米间距（默认 [`PIXELS_PER_CM`]）。
pub fn ruler_mm_pos(start: Pos2, dir: Vec2, mm_index: usize, pixels_per_cm: f32) -> Pos2 {
    start + dir * (mm_index as f32 * pixels_per_cm / 10.0)
}

// ─────────────────────────────────────────────────────────────────────────────
// 绘制
// ─────────────────────────────────────────────────────────────────────────────

/// 圆规：虚线圆预览 + 两臂 + 转轴/铅笔脚标记 + 半径文本。
pub fn draw_compass(painter: &Painter, t: &CompassTool) {
    let r = t.radius();
    let pivot = t.pivot;
    let pencil = t.pencil;

    // 预览圆（浅灰，未拖动时半径可能为 0）。
    if r > 1.0 {
        painter.circle_stroke(pivot, r, Stroke::new(1.0, Color32::from_gray(140)));
    }
    // 圆心十字标记。
    let cross = 6.0;
    painter.line_segment(
        [pivot - Vec2::new(cross, 0.0), pivot + Vec2::new(cross, 0.0)],
        Stroke::new(1.0, Color32::from_rgb(0, 150, 255)),
    );
    painter.line_segment(
        [pivot - Vec2::new(0.0, cross), pivot + Vec2::new(0.0, cross)],
        Stroke::new(1.0, Color32::from_rgb(0, 150, 255)),
    );
    // 两臂（转轴 → 铅笔脚）。
    painter.line_segment([pivot, pencil], Stroke::new(2.0, Color32::from_rgb(20, 40, 120)));
    painter.circle_filled(pivot, 4.0, Color32::from_rgb(20, 40, 120));
    painter.circle_filled(pencil, 3.0, Color32::from_rgb(180, 30, 30));

    if r > 1.0 {
        let mid = pivot.lerp(pencil, 0.5);
        painter.text(
            mid,
            Align2::CENTER_CENTER,
            format!("r={r:.1}"),
            FontId::proportional(14.0),
            Color32::from_rgb(20, 120, 40),
        );
        // Arc / Sector 模式：显示起止角。
        if t.mode != CompassMode::Circle {
            painter.text(
                pivot + Vec2::new(0.0, -r - 24.0),
                Align2::CENTER_CENTER,
                format!("{:.0}°…{:.0}°", t.arc_start_deg, t.arc_end_deg),
                FontId::proportional(13.0),
                Color32::from_rgb(20, 120, 40),
            );
        }
    }
}

/// 三角尺：半透明三角形 + 边框。
pub fn draw_set_square(painter: &Painter, t: &SetSquareTool) {
    let pts = set_square_points(t);
    painter.add(egui::Shape::convex_polygon(
        pts.to_vec(),
        SETSQUARE_FILL,
        Stroke::new(1.0, SETSQUARE_GRAY),
    ));
    // 直角标记（小方块）。
    let dir = rotate_vec(Vec2::new(1.0, 0.0), t.rotation_deg.to_radians());
    let perp = Vec2::new(-dir.y, dir.x);
    let m = 10.0;
    let o = t.origin;
    let p3 = o + dir * m + perp * m;
    painter.add(egui::Shape::convex_polygon(
        vec![o + dir * m, p3, o + perp * m],
        Color32::TRANSPARENT,
        Stroke::new(1.0, SETSQUARE_GRAY),
    ));
    // 旋转角文本。
    painter.text(
        t.origin + Vec2::new(0.0, -16.0),
        Align2::CENTER_CENTER,
        format!("{:.0}°", t.rotation_deg),
        FontId::proportional(13.0),
        SETSQUARE_TEXT_GRAY,
    );

    // 旋转手柄（重心）：灰色圆点 + 外圈，旋转拖拽时高亮为亮蓝。
    let grip = set_square_centroid(t);
    if t.rotating {
        painter.circle_stroke(grip, 10.0, Stroke::new(2.0, Color32::from_rgb(0, 150, 255)));
        painter.circle_filled(grip, 5.0, Color32::from_rgb(0, 150, 255));
    } else {
        painter.circle_stroke(grip, 8.0, Stroke::new(1.0, SETSQUARE_TICK_GRAY));
        painter.circle_filled(grip, 4.0, SETSQUARE_GRAY);
    }

    // 三个顶点手柄：浅灰小圆点（拖拽任一顶点均可绕 origin 旋转）。
    for v in pts.iter() {
        painter.circle_filled(*v, 3.0, SETSQUARE_TICK_GRAY);
    }

    // 沿线画线：虚线预览线段 + 长度文本。
    if t.drawing {
        let len = t.line_start.distance(t.line_current);
        painter.extend(egui::Shape::dashed_line(
            &[t.line_start, t.line_current],
            Stroke::new(2.0, Color32::from_rgb(255, 0, 0)),
            8.0,
            4.0,
        ));
        if len > 2.0 {
            let mid = t.line_start.lerp(t.line_current, 0.5);
            painter.text(
                mid + Vec2::new(0.0, -12.0),
                Align2::CENTER_CENTER,
                format!("len={len:.1}"),
                FontId::proportional(13.0),
                Color32::RED,
            );
        }
    }
}

/// 量角器：半圆盘 + 0°–180° 刻度 + 跟随鼠标的射线 + 角度文本。
pub fn draw_protractor(painter: &Painter, t: &ProtractorTool) {
    let c = t.center;
    let r = t.radius;
    let base_color = Color32::from_gray(130);

    // 半圆盘（上半圆，用 circle_stroke 画整圆再画基线，视觉即半圆量角器）。
    painter.circle_stroke(c, r, Stroke::new(1.0, base_color));
    // 基线（直径，恒为水平，不再随 rotation_deg 旋转）。
    let left = c + Vec2::new(-r, 0.0);
    let right = c + Vec2::new(r, 0.0);
    painter.line_segment([left, right], Stroke::new(1.0, base_color));

    // 刻度：0°–180°，每 10° 中刻度，每 30° 长刻度 + 数字。
    for deg in (0..=180).step_by(10) {
        // 量角器读数 deg → 统一角度（0°=正右）。
        let a = protractor_to_unified(deg as f32).to_radians();
        let dir = Vec2::new(a.cos(), -a.sin());
        let is_major = deg % 30 == 0;
        let tick = if is_major { 12.0 } else { 6.0 };
        let p1 = c + dir * r;
        let p2 = c + dir * (r - tick);
        painter.line_segment([p1, p2], Stroke::new(1.0, base_color));
        if is_major {
            let lp = c + dir * (r + 14.0);
            painter.text(
                lp,
                Align2::CENTER_CENTER,
                format!("{deg}"),
                FontId::proportional(10.0),
                Color32::from_gray(160),
            );
        }
    }

    // 跟随鼠标的射线。
    let a = protractor_to_unified(t.cursor_angle_deg).to_radians();
    let ray_end = c + Vec2::new(a.cos(), -a.sin()) * r;
    painter.line_segment([c, ray_end], Stroke::new(1.0, Color32::RED));

    // 画角模式：若已点第一条边，画第一条边。
    if let Some(first) = t.first_angle_deg {
        let a = protractor_to_unified(first).to_radians();
        let p = c + Vec2::new(a.cos(), -a.sin()) * r;
        painter.line_segment([c, p], Stroke::new(2.0, Color32::from_rgb(0, 150, 255)));
    }

    // 角度文本。
    painter.text(
        c + Vec2::new(0.0, -r - 22.0),
        Align2::CENTER_CENTER,
        format!("θ = {:.0}°", t.cursor_angle_deg),
        FontId::proportional(14.0),
        Color32::RED,
    );

    // 圆心（角度顶点）：深色圆点 + 外圈；拖拽移动时高亮为亮蓝。
    if t.dragging {
        painter.circle_stroke(c, 10.0, Stroke::new(2.0, Color32::from_rgb(0, 150, 255)));
        painter.circle_filled(c, 5.0, Color32::from_rgb(0, 150, 255));
    } else {
        painter.circle_stroke(c, 8.0, Stroke::new(1.0, Color32::from_gray(160)));
        painter.circle_filled(c, 4.0, Color32::from_rgb(20, 40, 120));
    }
}

/// 直尺三级刻度数量（纯几何）：返回 `(cm 主刻度线数, 半 cm 中刻度线数, mm 次刻度线数)`，
/// 供 [`draw_ruler_ticks`] 绘制与刻度数量测试复用。
///
/// - cm 主刻度遍历 `0..=cm_count`（含两端），故线数 = cm_count + 1；
/// - 半 cm 中刻度：half 网格 `1..whole` 中奇数（即 cm+5mm 处）；
/// - mm 次刻度：`1..whole` 中跳过 5 的倍数（整 cm 与 半 cm）。
pub fn ruler_scale_counts(length: f32, pixels_per_cm: f32) -> (usize, usize, usize) {
    let mm_step = pixels_per_cm / 10.0;
    let half_cm_step = pixels_per_cm / 2.0;
    let mm_count = (length / mm_step) as usize;
    let half_cm_count = (length / half_cm_step) as usize;
    let cm_count = (length / pixels_per_cm) as usize;

    let cm_lines = cm_count + 1;
    let half_cm_lines = (1..half_cm_count).filter(|i| i % 2 != 0).count();
    let mm_lines = (1..mm_count).filter(|i| i % 5 != 0).count();
    (cm_lines, half_cm_lines, mm_lines)
}

/// 直尺尺身厚度（测量线向下延伸，px）。
const RULER_BODY_DEPTH: f32 = 26.0;

/// 直尺三级刻度（mm 次 / 半 cm 中 / cm 主 + 数字）。
///
/// `start`–`end` 为直尺测量线（尺身顶边）；刻度沿 `-perp` 方向（垂直测量线、朝尺身内）延伸，
/// 数字用 `Align2::CENTER_BOTTOM` 落于主刻度下方。
pub fn draw_ruler_ticks(
    painter: &Painter,
    start: Pos2,
    end: Pos2,
    pixels_per_cm: f32,
) {
    let dir = (end - start).normalized();
    let perp = dir.rot90();
    let length = start.distance(end);
    if length <= 0.0 {
        return;
    }

    let (cm_lines, half_cm_lines, mm_lines) = ruler_scale_counts(length, pixels_per_cm);

    // --- mm 次刻度：第 j 根（0 起）的毫米序数 = j+1 + j/4（跳过每第 5 个刻度）。---
    for j in 0..mm_lines {
        let pos = ruler_mm_pos(start, dir, j + 1 + j / 4, pixels_per_cm);
        painter.line_segment([pos, pos - perp * 4.0], Stroke::new(1.0, SETSQUARE_TICK_GRAY));
    }

    // --- 半 cm 中刻度：half 网格奇数序数（2k+1 → 毫米序数 ×5）。---
    for k in 0..half_cm_lines {
        let pos = ruler_mm_pos(start, dir, (2 * k + 1) * 5, pixels_per_cm);
        painter.line_segment([pos, pos - perp * 7.0], Stroke::new(1.0, SETSQUARE_TICK_GRAY));
    }

    // --- cm 主刻度 + 数字（0 起，每 cm 一个，毫米序数 ×10）。---
    for i in 0..cm_lines {
        let pos = ruler_mm_pos(start, dir, i * 10, pixels_per_cm);
        painter.line_segment([pos, pos - perp * 10.0], Stroke::new(1.5, SETSQUARE_GRAY));
        if i > 0 {
            painter.text(
                pos - perp * 14.0,
                Align2::CENTER_BOTTOM,
                i.to_string(),
                FontId::proportional(10.0),
                SETSQUARE_TEXT_GRAY,
            );
        }
    }
}

/// 直尺：半透明灰白尺身 + mm/cm 双级刻度 + 端点手柄 + 长度标注。
pub fn draw_ruler(painter: &Painter, t: &RulerTool) {
    let dir = ruler_dir(t);
    let perp = dir.rot90();

    // 尺身矩形：测量线（start→end）为顶边，沿 -perp（垂直测量线）向下延伸。
    let body = [
        t.start,
        t.end,
        t.end - perp * RULER_BODY_DEPTH,
        t.start - perp * RULER_BODY_DEPTH,
    ];
    painter.add(egui::Shape::convex_polygon(
        body.to_vec(),
        SETSQUARE_FILL,
        Stroke::new(2.0, SETSQUARE_GRAY),
    ));

    // 刻度（尺身之后、手柄之前）。
    draw_ruler_ticks(painter, t.start, t.end, PIXELS_PER_CM);

    // 端点手柄：默认灰、拖拽中亮蓝高亮。
    let start_active = matches!(t.dragging_end, Some(WhichEnd::Start));
    let end_active = matches!(t.dragging_end, Some(WhichEnd::End));
    for (p, active) in [(t.start, start_active), (t.end, end_active)] {
        if active {
            painter.circle_stroke(p, 9.0, Stroke::new(2.0, Color32::from_rgb(0, 150, 255)));
            painter.circle_filled(p, 7.0, Color32::from_rgb(0, 150, 255));
        } else {
            painter.circle_filled(p, 5.0, SETSQUARE_GRAY);
        }
    }

    // 长度标注：尺身下缘居中显示总长（cm）。
    let mid = t.start.lerp(t.end, 0.5);
    let len_cm = t.start.distance(t.end) / PIXELS_PER_CM;
    painter.text(
        mid - perp * (RULER_BODY_DEPTH + 10.0),
        Align2::CENTER_CENTER,
        format!("L = {len_cm:.1} cm"),
        FontId::proportional(14.0),
        SETSQUARE_TEXT_GRAY,
    );
}

/// 正多边形：半透明填充 + 描边预览 + 边数/半径文本 + 中心标记。
pub fn draw_polygon(painter: &Painter, t: &PolygonTool) {
    if t.radius > 1.0 {
        let pts = crate::shape_renderer::polygon_vertices(t.center, t.radius, t.sides, t.preview_angle);
        painter.add(egui::Shape::convex_polygon(
            pts,
            Color32::from_rgba_unmultiplied(0, 150, 255, 60),
            Stroke::new(2.0, Color32::from_rgb(0, 150, 255)),
        ));
        painter.text(
            t.center + Vec2::new(0.0, -t.radius - 20.0),
            Align2::CENTER_CENTER,
            format!("{}边 r={:.1}", t.sides, t.radius),
            FontId::proportional(13.0),
            Color32::from_rgb(20, 120, 40),
        );
    }
    // 中心十字 + 圆点（未定位时提示点击位置）。
    let cross = 6.0;
    painter.line_segment(
        [t.center - Vec2::new(cross, 0.0), t.center + Vec2::new(cross, 0.0)],
        Stroke::new(1.0, Color32::from_rgb(0, 150, 255)),
    );
    painter.line_segment(
        [t.center - Vec2::new(0.0, cross), t.center + Vec2::new(0.0, cross)],
        Stroke::new(1.0, Color32::from_rgb(0, 150, 255)),
    );
    painter.circle_filled(t.center, 4.0, Color32::from_rgb(0, 150, 255));
}

/// 函数绘图：坐标系（X/Y 轴 + 刻度 + 网格）+ 表达式曲线预览。
pub fn draw_function_plot(painter: &Painter, t: &FunctionPlotTool) {
    let c = t.center;
    let s = t.scale;
    let half_units = 10.0; // 显示 -10..=10 单位范围
    let axis_color = Color32::from_rgb(90, 90, 90);
    let grid_color = Color32::from_rgba_unmultiplied(150, 150, 150, 60);
    let axis_stroke = Stroke::new(1.5, axis_color);

    // 网格（每 1 单位浅线）。
    for i in -(half_units as i32)..=half_units as i32 {
        let u = i as f32;
        let x = c.x + u * s;
        let y = c.y + u * s;
        // 竖线（u 在 x 轴，即数学 y 方向在屏幕水平）
        painter.line_segment(
            [Pos2::new(x, c.y - half_units * s), Pos2::new(x, c.y + half_units * s)],
            Stroke::new(1.0, grid_color),
        );
        painter.line_segment(
            [Pos2::new(c.x - half_units * s, y), Pos2::new(c.x + half_units * s, y)],
            Stroke::new(1.0, grid_color),
        );
    }

    // X 轴（水平，数学 y=0 → 屏幕 c.y）。
    painter.line_segment(
        [Pos2::new(c.x - half_units * s, c.y), Pos2::new(c.x + half_units * s, c.y)],
        axis_stroke,
    );
    // Y 轴（垂直，数学 x=0 → 屏幕 c.x；数学 y 向上 = 屏幕 y 减小）。
    painter.line_segment(
        [Pos2::new(c.x, c.y - half_units * s), Pos2::new(c.x, c.y + half_units * s)],
        axis_stroke,
    );
    // 轴箭头。
    let arrow = |painter: &Painter, tip: Pos2, from: Pos2| {
        let dir = (tip - from).normalized();
        let perp = Vec2::new(-dir.y, dir.x);
        painter.line_segment([tip, tip - dir * 8.0 + perp * 4.0], axis_stroke);
        painter.line_segment([tip, tip - dir * 8.0 - perp * 4.0], axis_stroke);
    };
    arrow(painter, Pos2::new(c.x + half_units * s, c.y), Pos2::new(c.x + (half_units - 1.0) * s, c.y));
    arrow(painter, Pos2::new(c.x, c.y - half_units * s), Pos2::new(c.x, c.y - (half_units - 1.0) * s));

    // 刻度标注（每 2 单位数字，数学坐标）。
    for i in (-half_units as i32..=half_units as i32).step_by(2) {
        let u = i as f32;
        if u != 0.0 {
            painter.text(
                Pos2::new(c.x + u * s, c.y + 14.0),
                Align2::CENTER_CENTER,
                format!("{u:.0}"),
                FontId::proportional(10.0),
                Color32::from_gray(160),
            );
            painter.text(
                Pos2::new(c.x - 14.0, c.y - u * s),
                Align2::CENTER_CENTER,
                format!("{u:.0}"),
                FontId::proportional(10.0),
                Color32::from_gray(160),
            );
        }
    }

    // 曲线：200 个采样点连线（跳过非有限值）。
    if let Some(expr) = &t.parsed {
        let pts = crate::function_parser::sample_points(expr, -half_units, half_units, 200);
        let curve_stroke = Stroke::new(2.0, Color32::from_rgb(0, 150, 255));
        for w in pts.windows(2) {
            let (x0, y0) = w[0];
            let (x1, y1) = w[1];
            let a = Pos2::new(c.x + x0 * s, c.y - y0 * s);
            let b = Pos2::new(c.x + x1 * s, c.y - y1 * s);
            painter.line_segment([a, b], curve_stroke);
        }
    }

    // 表达式 + 错误提示。
    if let Some(err) = &t.error {
        painter.text(
            c + Vec2::new(0.0, half_units * s + 30.0),
            Align2::CENTER_CENTER,
            format!("表达式错误: {err}"),
            FontId::proportional(13.0),
            Color32::from_rgb(220, 60, 60),
        );
    } else if !t.expr_str.is_empty() {
        painter.text(
            c + Vec2::new(0.0, half_units * s + 30.0),
            Align2::CENTER_CENTER,
            format!("y = {}", t.expr_str),
            FontId::proportional(13.0),
            Color32::from_rgb(0, 150, 255),
        );
    }
}

/// 数轴教具预览：主线 + 末端箭头 + 等距刻度 + 数字（交互态）。
pub fn draw_number_line_tool(painter: &Painter, t: &NumberLineTool) {
    let body = Color32::from_gray(60);
    let rect = egui::Rect::from_two_pos(t.start, t.end);
    let step = t.step.max(2.0);
    let (a, b, ticks) = crate::shape_renderer::number_line_geometry(rect, step);
    let stroke = Stroke::new(2.0, body);
    painter.line_segment([a, b], stroke);

    // 末端箭头。
    let dir = (b - a).normalized();
    let perp = Vec2::new(-dir.y, dir.x);
    let head = 8.0;
    painter.line_segment([b, b - dir * head + perp * head * 0.5], stroke);
    painter.line_segment([b, b - dir * head - perp * head * 0.5], stroke);

    // 刻度 + 数字（每 label_interval 个标一个，从 0 起）。
    let tick_len = 6.0;
    for (i, off) in ticks.iter().enumerate() {
        let base = a + dir * (*off);
        let tip = base + perp * tick_len;
        painter.line_segment([base, tip], Stroke::new(1.0, body));
        if t.label_interval > 0 && i % t.label_interval as usize == 0 {
            let label_pos = base + perp * (tick_len + 12.0);
            painter.text(
                label_pos,
                Align2::CENTER_CENTER,
                format!("{i}"),
                FontId::proportional(11.0),
                body,
            );
        }
    }

    // 端点：起点深蓝实心；拖拽中终点蓝色高亮圆环。
    painter.circle_filled(t.start, 4.0, Color32::from_rgb(20, 40, 120));
    if t.dragging {
        painter.circle_stroke(t.end, 6.0, Stroke::new(2.0, Color32::from_rgb(0, 150, 255)));
        painter.circle_filled(t.end, 3.0, Color32::from_rgb(0, 150, 255));
    } else {
        painter.circle_filled(t.end, 4.0, Color32::from_rgb(20, 40, 120));
    }
}

/// 绘制当前激活教具（覆盖在最上层）。
///
/// `time` 为 `ctx.input(|i| i.time)`，仅倒计时器用（驱动到 0 后闪烁），其余忽略。
pub fn draw_active_tool(painter: &Painter, tool: &ActiveTool, time: f64) {
    match tool {
        ActiveTool::None => {}
        ActiveTool::Compass(c) => draw_compass(painter, c),
        ActiveTool::SetSquare(s) => draw_set_square(painter, s),
        ActiveTool::Protractor(p) => draw_protractor(painter, p),
        ActiveTool::Ruler(r) => draw_ruler(painter, r),
        ActiveTool::Polygon(p) => draw_polygon(painter, p),
        ActiveTool::FunctionPlot(f) => draw_function_plot(painter, f),
        ActiveTool::NumberLine(n) => draw_number_line_tool(painter, n),
        ActiveTool::Countdown(c) => draw_countdown(painter, c, time),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 单元测试（纯几何）
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_angle_attracts_nearby() {
        let (a, snapped) = snap_angle(32.0, 3.0);
        assert_eq!(a, 30.0);
        assert!(snapped);
    }

    #[test]
    fn snap_angle_out_of_tolerance_returns_raw() {
        let (a, snapped) = snap_angle(37.0, 3.0);
        assert_eq!(a, 37.0);
        assert!(!snapped);
    }

    #[test]
    fn snap_angle_exact() {
        let (a, _) = snap_angle(90.0, 3.0);
        assert_eq!(a, 90.0);
    }

    #[test]
    fn protractor_roundtrip() {
        // 量角器读数 45° ↔ 统一角度 135°（正右起逆时针）。
        assert!((protractor_to_unified(45.0) - 135.0).abs() < 1e-4);
        assert!((unified_to_protractor(135.0) - 45.0).abs() < 1e-4);
        // 量角器 0° = 正左（统一 180°），180° = 正右（统一 0°）。
        assert!((unified_to_protractor(0.0) - 180.0).abs() < 1e-4);
        assert!((unified_to_protractor(180.0) - 0.0).abs() < 1e-4);
        assert!((unified_to_protractor(90.0) - 90.0).abs() < 1e-4);
    }

    #[test]
    fn set_square_45_has_right_angle() {
        let t = SetSquareTool {
            kind: SetSquareKind::Triangle45_45_90,
            origin: Pos2::new(100.0, 100.0),
            rotation_deg: 0.0,
            size: 100.0,
            moving: false,
            drawing: false,
            rotating: false,
            line_start: Pos2::ZERO,
            line_current: Pos2::ZERO,
            line_edge: None,
        };
        let pts = set_square_points(&t);
        assert_eq!(pts[0], Pos2::new(100.0, 100.0));
        assert_eq!(pts[1], Pos2::new(200.0, 100.0));
        assert_eq!(pts[2], Pos2::new(200.0, 200.0));
    }

    #[test]
    fn compass_radius() {
        let t = CompassTool {
            pivot: Pos2::new(0.0, 0.0),
            pencil: Pos2::new(30.0, 40.0),
            mode: CompassMode::Circle,
            arc_start_deg: 0.0,
            arc_end_deg: 90.0,
            stage: 1,
        };
        assert!((t.radius() - 50.0).abs() < 1e-4);
    }

    /// 三角尺旋转吸附：32° → 30°（±3° 内吸附）。
    #[test]
    fn setsquare_snap_angle() {
        let (a, snapped) = snap_angle(32.0, 3.0);
        assert_eq!(a, 30.0);
        assert!(snapped);
        let (a, _) = snap_angle(88.0, 3.0);
        assert_eq!(a, 90.0);
        let (a, _) = snap_angle(46.0, 3.0);
        assert_eq!(a, 45.0);
    }

    /// 量角器测量：鼠标在正右上 45° 位置 → cursor_angle_deg ≈ 45°。
    #[test]
    fn protractor_measure_angle() {
        let center = Pos2::new(100.0, 100.0);
        // 统一角度 135°（正左上）= 量角器读数 45°。
        let mouse = Pos2::new(100.0 - 70.7106, 100.0 - 70.7106);
        let unified = angle_of(center, mouse);
        let cursor = unified_to_protractor(unified);
        assert!((cursor - 45.0).abs() < 1.0, "cursor={cursor}");
        // 正上方 = 量角器 90°。
        let mouse = Pos2::new(100.0, 0.0);
        let cursor = unified_to_protractor(angle_of(center, mouse));
        assert!((cursor - 90.0).abs() < 1.0, "cursor={cursor}");
    }

    /// 角度向量计算：正右 = 0°、正上 = 90°、右下 = -45°、正左 = 180°。
    #[test]
    fn angle_of_quadrants() {
        let c = Pos2::new(0.0, 0.0);
        assert!((angle_of(c, Pos2::new(100.0, 0.0)) - 0.0).abs() < 1e-4);
        assert!((angle_of(c, Pos2::new(0.0, -100.0)) - 90.0).abs() < 1e-4);
        assert!((angle_of(c, Pos2::new(100.0, 100.0)) - (-45.0)).abs() < 1e-4);
        // 正左：atan2 有符号零使结果可能是 ±180°（数学上等价），取绝对值判定。
        assert!((angle_of(c, Pos2::new(-100.0, 0.0)).abs() - 180.0).abs() < 1e-4);
    }

    /// Shift + 拖拽的 15° 网格吸附（含负角度）。
    #[test]
    fn snap_angle_grid15_steps() {
        assert!((snap_angle_grid15(13.0) - 15.0).abs() < 1e-4);
        assert!((snap_angle_grid15(22.0) - 15.0).abs() < 1e-4);
        assert!((snap_angle_grid15(52.0) - 45.0).abs() < 1e-4);
        assert!((snap_angle_grid15(97.0) - 90.0).abs() < 1e-4);
        assert!((snap_angle_grid15(-48.0) - (-45.0)).abs() < 1e-4);
    }

    /// 三角尺重心 = 三顶点坐标平均。
    #[test]
    fn set_square_centroid_average() {
        let t = SetSquareTool {
            kind: SetSquareKind::Triangle45_45_90,
            origin: Pos2::new(100.0, 100.0),
            rotation_deg: 0.0,
            size: 100.0,
            moving: false,
            drawing: false,
            rotating: false,
            line_start: Pos2::ZERO,
            line_current: Pos2::ZERO,
            line_edge: None,
        };
        let c = set_square_centroid(&t);
        // 顶点 (100,100)(200,100)(200,200) → 重心 (166.67, 133.33)。
        assert!((c.x - 166.666_7).abs() < 0.01);
        assert!((c.y - 133.333_3).abs() < 0.01);
    }

    /// 量角器旋转吸附：鼠标在约 32° 方向拖拽 + Shift → 吸附到 30°。
    #[test]
    fn protractor_rotation_snap() {
        let center = Pos2::new(0.0, 0.0);
        // 32°（统一角，0°=正右、y 向下）：cos32≈0.8480，sin32≈0.5299。
        let mouse = Pos2::new(84.804, -52.992);
        let raw = angle_of(center, mouse);
        assert!((raw - 32.0).abs() < 1.0, "raw={raw}");
        assert!((snap_angle_grid15(raw) - 30.0).abs() < 1e-4);
    }

    /// 三角尺 45-45-90 构造（供旋转/几何测试复用）。
    fn set_square_45() -> SetSquareTool {
        SetSquareTool {
            kind: SetSquareKind::Triangle45_45_90,
            origin: Pos2::new(100.0, 100.0),
            rotation_deg: 0.0,
            size: 100.0,
            moving: false,
            drawing: false,
            rotating: false,
            line_start: Pos2::ZERO,
            line_current: Pos2::ZERO,
            line_edge: None,
        }
    }

    /// 方向修正：鼠标在 origin 正下方拖拽 → -angle_of = +90°，短边（p2）应指向下方；
    /// Shift 吸附后角度仍为 15° 整数倍。
    #[test]
    fn setsquare_rotation_follows_mouse_down() {
        let mut t = set_square_45();
        let mouse = Pos2::new(100.0, 200.0); // origin(100,100) 正下方
        let raw = -angle_of(t.origin, mouse);
        assert!((raw - 90.0).abs() < 1e-3, "raw={raw}");
        t.rotation_deg = snap_angle_grid15(raw);
        assert!((t.rotation_deg - 90.0).abs() < 1e-4);
        let pts = set_square_points(&t);
        // 短边端点 p2 应位于 origin 正下方（x 不变、y 变大）。
        assert!((pts[1].x - t.origin.x).abs() < 1e-2);
        assert!(pts[1].y > t.origin.y + 50.0);
        // 角度为 15° 整数倍。
        assert!((t.rotation_deg % 15.0).abs() < 1e-4);
    }

    /// 边缘吸附：靠近某条边时命中最近边并返回吸附点；远离时返回 `None`。
    #[test]
    fn test_edge_snap_detects_near_edge() {
        let t = set_square_45();
        // 底边 (100,100)-(200,100) 上方 5px 处 → 命中边 0，吸附点 (150,100)。
        let (edge_idx, snap) =
            find_nearest_edge(Pos2::new(150.0, 95.0), &t, 8.0).expect("应命中底边");
        assert_eq!(edge_idx, 0);
        assert!((snap.x - 150.0).abs() < 1e-3);
        assert!((snap.y - 100.0).abs() < 1e-3);
        // 远离三角形 → None。
        assert!(find_nearest_edge(Pos2::new(500.0, 500.0), &t, 8.0).is_none());
    }

    /// 沿边画线提交：命中边 → 吸附起点 → 沿边延长 → 长度超过阈值时提交线段；极短拖拽防误触。
    #[test]
    fn test_line_drawing_submits_line() {
        let t = set_square_45();
        // 靠近底边按下 → 命中边 0 并吸附起点。
        let (edge_idx, start) =
            find_nearest_edge(Pos2::new(120.0, 96.0), &t, 8.0).expect("命中底边");
        assert_eq!(edge_idx, 0);
        // 拖动到边上更远处（吸附到所选边方向），构造终点。
        let (a, b) = set_square_edges(&t)[edge_idx];
        let current = closest_point_on_line(Pos2::new(220.0, 99.0), a, b);
        // 起止距离 > 5px → 提交线段。
        let (p0, p1) = line_draw_result(start, current, 5.0).expect("应提交线段");
        assert_eq!(p0, start);
        assert_eq!(p1, current);
        assert!(p0.distance(p1) > 5.0);
        // 极短拖拽 → 防误触不提交。
        assert!(line_draw_result(start, Pos2::new(start.x + 3.0, start.y), 5.0).is_none());
    }

    /// 直尺刻度位置：0mm → start、每 mm 均匀间隔、整 cm 与总长对齐。
    #[test]
    fn ruler_tick_positions_correct() {
        let t = RulerTool {
            start: Pos2::new(0.0, 0.0),
            end: Pos2::new(PIXELS_PER_CM * 5.0, 0.0), // 5cm 水平直尺
            dragging_end: None,
            dragging_body: false,
            last_mouse: Pos2::ZERO,
        };
        let dir = ruler_dir(&t);
        // 0mm = start。
        assert_eq!(ruler_mm_pos(t.start, dir, 0, PIXELS_PER_CM), t.start);
        // 10mm（1cm）= start + 37.8px。
        let one_cm = ruler_mm_pos(t.start, dir, 10, PIXELS_PER_CM);
        assert!((one_cm.x - PIXELS_PER_CM).abs() < 1e-3);
        assert!(one_cm.y.abs() < 1e-3);
        // 50mm（5cm）= end。
        let five_cm = ruler_mm_pos(t.start, dir, 50, PIXELS_PER_CM);
        assert!((five_cm.x - t.end.x).abs() < 1e-3);
        // 相邻毫米刻度间距 = PIXELS_PER_CM / 10 ≈ 3.78px。
        let a = ruler_mm_pos(t.start, dir, 1, PIXELS_PER_CM);
        let b = ruler_mm_pos(t.start, dir, 2, PIXELS_PER_CM);
        assert!((b.x - a.x - (PIXELS_PER_CM / 10.0)).abs() < 1e-3);
    }

    /// Shift 吸附：近水平方向吸附到水平、近竖直方向吸附到竖直，长度不变。
    #[test]
    fn ruler_snap_to_horizontal() {
        // 近水平（约 -2.86°）→ 吸附到水平。
        let h = snap_dir_grid45(Vec2::new(100.0, 5.0));
        assert!(h.y.abs() < 1e-3, "水平吸附失败: {h:?}");
        assert!((h.x - 100.0).abs() < 1.0, "长度误差: {h:?}");
        // 近竖直 → 吸附到竖直。
        let v = snap_dir_grid45(Vec2::new(1.0, 100.0));
        assert!(v.x.abs() < 1e-3, "竖直吸附失败: {v:?}");
        assert!((v.y - 100.0).abs() < 1.0, "长度误差: {v:?}");
    }

    /// 200px 长直尺 → 约 5 个主刻度（0,1,2,3,4,5 cm），三级刻度线的数量正确。
    #[test]
    fn test_ruler_tick_count_on_200px() {
        let len = 200.0;
        let ppcm = PIXELS_PER_CM;
        let (cm_lines, half_cm_lines, mm_lines) = ruler_scale_counts(len, ppcm);
        // cm 主刻度：cm_count = 5，遍历 0..=5 → 6 条线（含两端 "0..5 cm"）。
        assert_eq!(cm_lines, 6);
        // 半 cm 中刻度：half 网格 1..10 中奇数 {1,3,5,7,9} → 5 条。
        assert_eq!(half_cm_lines, 5);
        // mm 次刻度：1..52 中跳过 5 的倍数（10 处）→ 51 - 10 = 41 条。
        assert_eq!(mm_lines, 41);
        // 主刻度数 = cm_lines - 1 = 5。
        assert_eq!(cm_lines - 1, 5);
    }

    /// 倒计时器：`MM:SS` 格式化（纯函数）。
    #[test]
    fn countdown_format_minutes_seconds() {
        assert_eq!(format_mmss(0), "00:00");
        assert_eq!(format_mmss(59), "00:59");
        assert_eq!(format_mmss(330), "05:30");
        assert_eq!(format_mmss(5999), "99:59");
    }

    /// 倒计时器：tick 递减到 0 → finished；到 0 后按 time 红/白闪烁；
    /// 最后 10 秒红色；输入解析（M:SS / 纯秒 / 非法）。
    #[test]
    fn countdown_finish_flashes() {
        let mut t = CountdownTool {
            total_seconds: 5,
            remaining_seconds: 5,
            is_running: true,
            is_finished: false,
            position: Pos2::ZERO,
            input_text: "5".to_string(),
            last_tick: None,
            pending_press: None,
            dragging: false,
            last_mouse: Pos2::ZERO,
        };
        // 每秒递减 → 到 0 置 finished。
        for _ in 0..5 {
            t.tick();
        }
        assert!(t.is_finished, "递减到 0 后应 finished");
        assert_eq!(t.remaining_seconds, 0);
        assert!(!t.is_running);
        // 闪烁：time*2 取整奇偶切换红/白。
        assert_eq!(t.flash_color(0.0), Color32::RED);
        assert_eq!(t.flash_color(0.3), Color32::RED);
        assert_eq!(t.flash_color(0.5), Color32::WHITE);
        // 最后 10 秒（未到 0）红色。
        let mut u = t.clone();
        u.is_finished = false;
        u.is_running = true;
        u.remaining_seconds = 3;
        assert_eq!(u.flash_color(0.0), Color32::RED);
        // 输入解析：M:SS / 纯秒 / 非法。
        let mut v = CountdownTool {
            input_text: "5:30".to_string(),
            ..CountdownTool::default()
        };
        assert!(v.parse_input().is_ok());
        assert_eq!(v.total_seconds, 330);
        assert_eq!(v.remaining_seconds, 330);
        assert!(v.is_running, "确认后应自动开始");
        let mut w = CountdownTool {
            input_text: "90".to_string(),
            ..CountdownTool::default()
        };
        assert!(w.parse_input().is_ok());
        assert_eq!(w.total_seconds, 90);
        let mut x = CountdownTool {
            input_text: "abc".to_string(),
            ..CountdownTool::default()
        };
        assert!(x.parse_input().is_err(), "非法输入应返回 Err");
        let mut y = CountdownTool {
            input_text: "0".to_string(),
            ..CountdownTool::default()
        };
        assert!(y.parse_input().is_err(), "0 秒应返回 Err");
    }
}
