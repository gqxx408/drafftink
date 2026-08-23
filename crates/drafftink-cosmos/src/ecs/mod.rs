//! ECS（实体-组件-系统）模块
//!
//! 轻量级 ECS 实现，用于管理宇宙场景中的天体对象。
//! 采用并行数组架构，每个组件类型存储为独立的数组，
//! 相同索引对应同一实体。

pub mod components;
pub mod systems;

// 重新导出所有组件，方便外部使用
pub use components::{
    transform_to_point, Label, Material, MeshHandle, Orbit, PlanetInfo, Rotation, Transform,
};

// 重新导出所有系统函数
pub use systems::{
    camera_orbit_system, ease_in_out_quad, ease_out_cubic, lerp_vec3, orbit_system,
    rotation_system,
};
