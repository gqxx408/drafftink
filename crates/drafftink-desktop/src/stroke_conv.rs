//! 备授一体：授课端 `InkStroke` 与核心中性 `StrokeData` 之间的桥接转换。
//!
//! 两种 Stroke 类型分别定义在 `drafftink_edit` 与 `drafftink_display` 中，字段并不相同。
//! 通过 `drafftink_core::document::StrokeData`（无 egui 依赖）作为跨模块中性格式，
//! 既避免依赖环，又保证批注层数据可在两模式间无损同步。
//!
//! 该转换只发生在整合边界（状态传递），不进入任何核心渲染 / 几何逻辑。

use drafftink_core::document::StrokeData as CoreStroke;
use drafftink_display::annotation::{InkStroke, ToolType};
use uuid::Uuid;

/// 核心中性格式 → 授课端 `InkStroke`。
pub fn core_to_ink(s: &CoreStroke) -> InkStroke {
    InkStroke {
        id: Uuid::new_v4(),
        tool: match s.tool {
            0 => ToolType::Pen,
            1 => ToolType::Highlighter,
            _ => ToolType::Eraser,
        },
        color: s.color,
        thickness: s.thickness,
        points: s.points.iter().map(|p| (p[0], p[1])).collect(),
        timestamp_ms: 0,
    }
}

/// 授课端 `InkStroke` → 核心中性格式。
pub fn ink_to_core(s: &InkStroke) -> CoreStroke {
    CoreStroke {
        points: s.points.iter().map(|(x, y)| [*x, *y]).collect(),
        color: s.color,
        thickness: s.thickness,
        tool: match s.tool {
            ToolType::Pen => 0,
            ToolType::Highlighter => 1,
            ToolType::Eraser => 2,
        },
    }
}

/// 批量：`InkStroke` 切片 → 核心中性批注。
pub fn ink_vec_to_core(v: &[InkStroke]) -> Vec<CoreStroke> {
    v.iter().map(ink_to_core).collect()
}

/// 批量：核心中性批注 → `InkStroke` 切片。
pub fn core_vec_to_ink(v: &[CoreStroke]) -> Vec<InkStroke> {
    v.iter().map(core_to_ink).collect()
}
