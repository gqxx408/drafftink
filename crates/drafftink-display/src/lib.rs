//! Drafftink Display — 授课端库接口（供 drafftink-desktop 上层整合复用）。
//!
//! 仅暴露既有 `DisplayApp` 的公开入口与最小整合钩子，**不**包含任何核心逻辑改写。
//! 模块声明与 `main.rs` 保持一致（`plugin_install_dialog.rs` 为未被引用的遗留文件，
//! 不纳入库编译）。

pub mod annotation;
pub mod app;
pub mod interaction;
pub mod log_setup;
pub mod multi_page;
pub mod physics;
pub mod render;
pub mod workshop;

pub use app::DisplayApp;
