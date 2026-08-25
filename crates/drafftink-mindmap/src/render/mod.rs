//! 2D 渲染层（egui）
//!
//! 将布局算法输出的节点位置渲染为 egui 图形。
//! 支持连线绘制（贝塞尔曲线/直线/弧线）和节点绘制。

pub mod egui_render;
pub mod viewer;

pub use egui_render::MindMapRenderer;
pub use viewer::MindMapViewer;
