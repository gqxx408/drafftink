//! Geometry types registered by the Math module.
//!
//! These element types are registered with the host so that the document
//! model can serialize/deserialize them via `typetag`.

use serde::{Deserialize, Serialize};

/// 2D geometry element types that the Math module contributes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GeometryElement {
    Point {
        x: f32,
        y: f32,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    },
    Circle {
        cx: f32,
        cy: f32,
        r: f32,
    },
    Triangle {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x3: f32,
        y3: f32,
    },
    Rectangle {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
}

impl GeometryElement {
    /// Human-readable label in Chinese.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Point { .. } => "点",
            Self::Line { .. } => "线段",
            Self::Circle { .. } => "圆",
            Self::Triangle { .. } => "三角形",
            Self::Rectangle { .. } => "矩形",
        }
    }
}
