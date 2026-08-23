//! UI 模块：用户界面组件
//!
//! 提供控制面板、3D 标签渲染和 2D 地图视图等 UI 功能。

pub mod labels;
pub mod controls;
pub mod map_view;

// 重新导出主要类型和函数，方便外部使用
pub use controls::{ControlPanel, ViewMode};
pub use labels::{render_labels, render_single_label};
pub use map_view::render_map_view;
