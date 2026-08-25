//! 物理学科工具模块 —— 纯 egui 实现，零 WebView 依赖。
//!
//! 目标：内存占用 < 5MB，启动瞬间完成，比希沃流畅 300%。
//!
//! # 模块结构
//!
//! - `elements` — 5 种物理图元的数据结构和绘制逻辑
//! - `editor`   — 完整的编辑器 UI（工具栏 + 画布 + 交互 + 内存监控）
//!
//! # 快速开始
//!
//! ```ignore
//! use drafftink_display::physics::PhysicsEditor;
//!
//! let mut editor = PhysicsEditor::new();
//! // 在 egui 的 update 函数里：
//! editor.ui(ctx);
//! ```

pub mod editor;
pub mod elements;

pub use editor::PhysicsEditor;
