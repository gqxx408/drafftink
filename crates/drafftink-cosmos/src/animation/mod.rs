//! 动画模块：补间、插值等动画工具

pub mod lerp;

pub use lerp::{
    clamp01, ease_in_out_quad, ease_linear, ease_out_back, ease_out_cubic, lerp_f32, lerp_vec3,
    AnimationController,
};
