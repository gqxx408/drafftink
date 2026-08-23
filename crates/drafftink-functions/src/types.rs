//! 核心数据类型
//!
//! 纯数据结构，无 UI 依赖，可直接序列化。

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 函数曲线定义（可序列化）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCurve {
    /// 唯一 ID
    pub id: Uuid,
    /// 表达式字符串，如 "sin(x)*x"
    pub expression: String,
    /// 颜色 RGBA
    pub color: [u8; 4],
    /// 是否可见
    pub visible: bool,
    /// 参数列表（用于 f(x, a, b, ...) 形式）
    pub parameters: Vec<Parameter>,
}

impl FunctionCurve {
    pub fn new(expression: &str, color: [u8; 4]) -> Self {
        Self {
            id: Uuid::new_v4(),
            expression: expression.to_string(),
            color,
            visible: true,
            parameters: Vec::new(),
        }
    }
}

/// 参数定义（可序列化）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    /// 参数名（表达式中使用的变量名）
    pub name: String,
    /// 当前值
    pub value: f64,
    /// 最小值
    pub min: f64,
    /// 最大值
    pub max: f64,
    /// 步长
    pub step: f64,
}

impl Parameter {
    pub fn new(name: &str, value: f64, min: f64, max: f64) -> Self {
        Self {
            name: name.to_string(),
            value,
            min,
            max,
            step: (max - min) / 100.0,
        }
    }
}

/// 视口（世界坐标范围）
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Viewport {
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            x_min: -10.0,
            x_max: 10.0,
            y_min: -5.0,
            y_max: 5.0,
        }
    }
}

impl Viewport {
    /// 世界坐标 X 范围宽度
    pub fn width(&self) -> f64 {
        self.x_max - self.x_min
    }

    /// 世界坐标 Y 范围高度
    pub fn height(&self) -> f64 {
        self.y_max - self.y_min
    }

    /// X 轴中心
    pub fn center_x(&self) -> f64 {
        (self.x_min + self.x_max) * 0.5
    }

    /// Y 轴中心
    pub fn center_y(&self) -> f64 {
        (self.y_min + self.y_max) * 0.5
    }

    /// 平移视口
    pub fn pan(&mut self, dx: f64, dy: f64) {
        self.x_min += dx;
        self.x_max += dx;
        self.y_min += dy;
        self.y_max += dy;
    }

    /// 以指定世界坐标点为中心缩放
    pub fn zoom_at(&mut self, world_x: f64, world_y: f64, factor: f64) {
        let factor = factor.clamp(0.1, 10.0);
        let new_x_min = world_x + (self.x_min - world_x) * factor;
        let new_x_max = world_x + (self.x_max - world_x) * factor;
        let new_y_min = world_y + (self.y_min - world_y) * factor;
        let new_y_max = world_y + (self.y_max - world_y) * factor;

        // 防止过度缩放
        let new_width = new_x_max - new_x_min;
        let new_height = new_y_max - new_y_min;
        if new_width > 0.001 && new_width < 100000.0 {
            self.x_min = new_x_min;
            self.x_max = new_x_max;
        }
        if new_height > 0.001 && new_height < 100000.0 {
            self.y_min = new_y_min;
            self.y_max = new_y_max;
        }
    }

    /// 重置为默认范围
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// 预设颜色调色板（8 种）
pub const CURVE_PALETTE: &[[u8; 4]] = &[
    [100, 180, 255, 255], // 蓝
    [255, 120, 100, 255], // 红
    [100, 220, 120, 255], // 绿
    [255, 200, 80, 255],  // 黄
    [200, 120, 255, 255], // 紫
    [80, 220, 220, 255],  // 青
    [255, 150, 200, 255], // 粉
    [180, 180, 180, 255], // 灰
];

/// 获取调色板中的第 n 种颜色（循环）
pub fn palette_color(index: usize) -> [u8; 4] {
    CURVE_PALETTE[index % CURVE_PALETTE.len()]
}
