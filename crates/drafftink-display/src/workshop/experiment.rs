//! 虚拟实验卡片 —— 基于 egui 的交互式实验模拟。
//!
//! 希沃用 WebView 加载 Flash/HTML5 实验，又重又慢。
//! 咱们直接用 Rust + egui 画交互元件，轻量且流畅。

use egui::{Color32, Pos2, Rect, Stroke};
use serde::{Deserialize, Serialize};

// ─── 实验类型 ──────────────────────────────────────────────────────────────

/// 实验类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExperimentType {
    /// 电路实验
    Circuit,
    /// 光学实验
    Optics,
    /// 力学实验
    Mechanics,
    /// 化学实验
    Chemistry,
}

#[allow(dead_code)]
impl ExperimentType {
    pub fn label(&self) -> &'static str {
        match self {
            ExperimentType::Circuit => "电路实验",
            ExperimentType::Optics => "光学实验",
            ExperimentType::Mechanics => "力学实验",
            ExperimentType::Chemistry => "化学实验",
        }
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            ExperimentType::Circuit => "⚡",
            ExperimentType::Optics => "💡",
            ExperimentType::Mechanics => "⚙️",
            ExperimentType::Chemistry => "🧪",
        }
    }
}

// ─── 实验卡片数据 ──────────────────────────────────────────────────────────

/// 虚拟实验卡片数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentCardData {
    /// 实验名称
    pub name: String,
    /// 实验描述
    pub description: String,
    /// 实验类型
    pub exp_type: ExperimentType,
    /// 难度等级（1-5）
    pub difficulty: u8,
    /// 实验步骤数
    pub step_count: usize,
    /// 是否已完成
    pub completed: bool,
}

// ─── 简单的电路实验模拟器 ──────────────────────────────────────────────────
//
// 为了演示，这里实现一个简单的"串联电路"可视化：
// - 电源、开关、灯泡、电阻
// - 开关可以点击切换开合
// - 闭合时灯泡"发光"，断开时熄灭
//
// 这只是最小 demo，后续可以扩展为完整的电路仿真引擎。

/// 电路元件类型。
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum CircuitComponent {
    /// 电源
    Battery { voltage: f32 },
    /// 开关
    Switch { closed: bool },
    /// 灯泡
    Bulb { lit: bool, resistance: f32 },
    /// 电阻
    Resistor { resistance: f32 },
}

/// 简单电路模拟器状态。
#[derive(Debug, Clone)]
pub struct SimpleCircuitState {
    /// 电源电压（V）
    pub voltage: f32,
    /// 总电阻（Ω）
    pub total_resistance: f32,
    /// 电流（A）
    pub current: f32,
    /// 开关是否闭合
    pub switch_closed: bool,
    /// 灯泡是否发光
    pub bulb_lit: bool,
}

impl Default for SimpleCircuitState {
    fn default() -> Self {
        Self {
            voltage: 3.0,
            total_resistance: 10.0,
            current: 0.0,
            switch_closed: false,
            bulb_lit: false,
        }
    }
}

impl SimpleCircuitState {
    /// 切换开关状态。
    pub fn toggle_switch(&mut self) {
        self.switch_closed = !self.switch_closed;
        self.recalculate();
    }

    /// 重新计算电路参数（欧姆定律）。
    fn recalculate(&mut self) {
        if self.switch_closed && self.total_resistance > 0.0 {
            self.current = self.voltage / self.total_resistance;
            self.bulb_lit = self.current > 0.0;
        } else {
            self.current = 0.0;
            self.bulb_lit = false;
        }
    }
}

/// 绘制一个简单的串联电路示意图。
///
/// 布局（从左到右）：
/// ```text
///   ──电池──开关──灯泡──电阻──
///   │                      │
///   └────────导线──────────┘
/// ```
#[allow(dead_code)]
pub fn draw_circuit_diagram(
    painter: &egui::Painter,
    rect: Rect,
    state: &SimpleCircuitState,
) {
    let stroke_color = if state.bulb_lit {
        Color32::from_rgb(255, 180, 0)
    } else {
        Color32::from_rgb(60, 60, 60)
    };
    let stroke = Stroke::new(2.5, stroke_color);

    let left = rect.left() + 20.0;
    let right = rect.right() - 20.0;
    let top = rect.top() + rect.height() * 0.3;
    let bottom = rect.top() + rect.height() * 0.7;
    let _mid_y = (top + bottom) / 2.0;

    // 四个元件均匀分布在顶部导线上
    let comp_count = 4;
    let spacing = (right - left) / (comp_count + 1) as f32;

    // 顶部导线
    painter.line_segment([Pos2::new(left, top), Pos2::new(right, top)], stroke);

    // 底部导线
    painter.line_segment([Pos2::new(left, bottom), Pos2::new(right, bottom)], stroke);

    // 左侧导线
    painter.line_segment([Pos2::new(left, top), Pos2::new(left, bottom)], stroke);

    // 右侧导线
    painter.line_segment([Pos2::new(right, top), Pos2::new(right, bottom)], stroke);

    // 元件位置
    let comp_x: Vec<f32> = (1..=comp_count)
        .map(|i| left + spacing * i as f32)
        .collect();

    // 1. 电池（长短线表示正负极）
    let bat_x = comp_x[0];
    let bat_long = 14.0;
    let bat_short = 8.0;
    painter.line_segment(
        [Pos2::new(bat_x, top - bat_long / 2.0), Pos2::new(bat_x, top + bat_long / 2.0)],
        stroke,
    );
    painter.line_segment(
        [
            Pos2::new(bat_x - 6.0, top - bat_short / 2.0),
            Pos2::new(bat_x - 6.0, top + bat_short / 2.0),
        ],
        stroke,
    );

    // 2. 开关
    let sw_x = comp_x[1];
    if state.switch_closed {
        // 闭合：直线
        painter.line_segment([Pos2::new(sw_x - 10.0, top), Pos2::new(sw_x + 10.0, top)], stroke);
    } else {
        // 断开：斜线
        painter.line_segment(
            [Pos2::new(sw_x - 10.0, top), Pos2::new(sw_x + 8.0, top - 10.0)],
            stroke,
        );
    }
    // 开关的两个节点
    painter.circle_filled(Pos2::new(sw_x - 10.0, top), 3.0, stroke_color);
    painter.circle_filled(Pos2::new(sw_x + 10.0, top), 3.0, stroke_color);

    // 3. 灯泡
    let bulb_x = comp_x[2];
    let bulb_r = 12.0;
    let bulb_center = Pos2::new(bulb_x, top);

    // 灯泡圆圈
    if state.bulb_lit {
        // 发光效果：黄色填充 + 光晕
        painter.circle_filled(bulb_center, bulb_r, Color32::from_rgb(255, 230, 100));
        painter.circle_stroke(bulb_center, bulb_r + 4.0, Stroke::new(2.0, Color32::from_rgba_unmultiplied(255, 200, 0, 120)));
    } else {
        painter.circle_filled(bulb_center, bulb_r, Color32::WHITE);
    }
    painter.circle_stroke(bulb_center, bulb_r, stroke);

    // 灯泡内部十字（灯丝）
    painter.line_segment(
        [
            Pos2::new(bulb_x - bulb_r * 0.5, top),
            Pos2::new(bulb_x + bulb_r * 0.5, top),
        ],
        stroke,
    );

    // 4. 电阻（锯齿形）
    let res_x = comp_x[3];
    let res_w = 24.0;
    let res_h = 8.0;
    let teeth = 4;
    let tooth_w = res_w / teeth as f32;

    for i in 0..teeth {
        let x_start = res_x - res_w / 2.0 + i as f32 * tooth_w;
        let y_top = top - res_h;
        let _y_bottom = top + res_h;
        let mid_y = top;

        // 画一个锯齿（从下到上到下）
        painter.line_segment([Pos2::new(x_start, mid_y), Pos2::new(x_start + tooth_w * 0.5, y_top)], stroke);
        painter.line_segment([Pos2::new(x_start + tooth_w * 0.5, y_top), Pos2::new(x_start + tooth_w, mid_y)], stroke);
    }

    // 返回一个空的 response（后续可以加点击交互）
    painter.ctx().request_repaint(); // 动画效果需要重绘
}

/// 生成示例实验卡片数据。
pub fn sample_experiment_card() -> ExperimentCardData {
    ExperimentCardData {
        name: "串联电路探究".to_string(),
        description: "观察串联电路中开关、灯泡、电阻的关系".to_string(),
        exp_type: ExperimentType::Circuit,
        difficulty: 2,
        step_count: 5,
        completed: false,
    }
}
