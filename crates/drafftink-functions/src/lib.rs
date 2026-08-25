//! drafftink-functions — 高性能 2D 数学函数绘图引擎
//!
//! 基于 egui Painter (GPU 加速) 的实时函数绘图组件。
//! 支持表达式解析、动态采样、平移缩放交互和参数滑块。

pub mod expr;
pub mod renderer;
pub mod sampler;
pub mod types;
pub mod viewer;
pub mod viewport;

pub use viewer::FunctionViewer;
