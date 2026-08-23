//! 渲染模块：负责 3D 场景的绘制

pub mod camera;
pub mod renderer;

// 重新导出主要类型
pub use camera::{project_point, OrbitCamera};
pub use renderer::{RenderBatch, SceneRenderer};
