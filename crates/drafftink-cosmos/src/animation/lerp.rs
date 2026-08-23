//! 线性插值（lerp）与缓动工具
//!
//! 提供标量/向量线性插值、常用缓动函数以及动画控制器。

use nalgebra::Vector3;

// ---------------------------------------------------------------------------
// 线性插值
// ---------------------------------------------------------------------------

/// 标量线性插值。
///
/// `t = 0` 返回 `a`，`t = 1` 返回 `b`。
#[inline]
pub fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// 三维向量线性插值。
///
/// `t = 0` 返回 `a`，`t = 1` 返回 `b`。
#[inline]
pub fn lerp_vec3(a: Vector3<f32>, b: Vector3<f32>, t: f32) -> Vector3<f32> {
    a + (b - a) * t
}

// ---------------------------------------------------------------------------
// 缓动函数
// ---------------------------------------------------------------------------

/// 线性缓动：`f(t) = t`。
#[inline]
pub fn ease_linear(t: f32) -> f32 {
    t
}

/// 三次方缓出：`f(t) = 1 - (1 - t)^3`。
///
/// 动画开始快，结束慢。
#[inline]
pub fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

/// 二次方缓入缓出。
///
/// - `t < 0.5` 时加速：`f(t) = 2 * t^2`
/// - `t >= 0.5` 时减速：`f(t) = 1 - (-2t + 2)^2 / 2`
#[inline]
pub fn ease_in_out_quad(t: f32) -> f32 {
    if t < 0.5 {
        2.0 * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
    }
}

/// 回弹缓出（back ease out）。
///
/// 动画结束时略微超过目标值再回弹回来。
/// 使用标准参数 `c1 = 1.70158`，`c3 = c1 + 1`。
#[inline]
pub fn ease_out_back(t: f32) -> f32 {
    let c1 = 1.701_58;
    let c3 = c1 + 1.0;
    let t1 = t - 1.0;
    1.0 + c3 * t1.powi(3) + c1 * t1.powi(2)
}

/// 将值限制在 `[0, 1]` 区间内。
#[inline]
pub fn clamp01(t: f32) -> f32 {
    t.clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// 动画控制器
// ---------------------------------------------------------------------------

/// 通用动画控制器。
///
/// 跟踪动画的开始时间、持续时间、播放状态以及使用的缓动函数，
/// 可以在任意时刻查询动画进度（0-1，已应用缓动）或是否完成。
#[derive(Clone, Debug)]
pub struct AnimationController {
    /// 动画开始的时间戳（秒）。
    pub start_time: f64,
    /// 动画持续时间（秒）。
    pub duration: f32,
    /// 动画是否正在播放。
    pub playing: bool,
    /// 缓动函数，将线性进度 [0,1] 映射为缓动后的进度 [0,1]。
    pub ease_fn: fn(f32) -> f32,
}

impl AnimationController {
    /// 创建一个新的动画控制器。
    ///
    /// - `duration`: 动画持续时间（秒）
    /// - `ease_fn`: 缓动函数
    ///
    /// 初始状态为未播放，需要调用 [`start`](Self::start) 开始。
    pub fn new(duration: f32, ease_fn: fn(f32) -> f32) -> Self {
        Self {
            start_time: 0.0,
            duration,
            playing: false,
            ease_fn,
        }
    }

    /// 开始播放动画。
    ///
    /// - `current_time`: 当前时间戳（秒）
    pub fn start(&mut self, current_time: f64) {
        self.start_time = current_time;
        self.playing = true;
    }

    /// 返回当前动画进度（0-1），已应用缓动函数。
    ///
    /// 若动画未播放，返回 0.0；若已超过持续时间，返回 1.0。
    ///
    /// - `current_time`: 当前时间戳（秒）
    pub fn progress(&self, current_time: f64) -> f32 {
        if !self.playing {
            return 0.0;
        }
        let elapsed = (current_time - self.start_time) as f32;
        let t = if self.duration <= 0.0 {
            1.0
        } else {
            clamp01(elapsed / self.duration)
        };
        (self.ease_fn)(t)
    }

    /// 判断动画是否已完成（进度 >= 1.0）。
    ///
    /// - `current_time`: 当前时间戳（秒）
    pub fn is_done(&self, current_time: f64) -> bool {
        if !self.playing {
            return false;
        }
        if self.duration <= 0.0 {
            return true;
        }
        let elapsed = (current_time - self.start_time) as f32;
        elapsed >= self.duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn test_lerp_f32() {
        assert!(approx_eq(lerp_f32(0.0, 10.0, 0.0), 0.0, 1e-6));
        assert!(approx_eq(lerp_f32(0.0, 10.0, 0.5), 5.0, 1e-6));
        assert!(approx_eq(lerp_f32(0.0, 10.0, 1.0), 10.0, 1e-6));
    }

    #[test]
    fn test_lerp_vec3() {
        let a = Vector3::new(0.0, 0.0, 0.0);
        let b = Vector3::new(10.0, 20.0, 30.0);
        let r = lerp_vec3(a, b, 0.5);
        assert!(approx_eq(r.x, 5.0, 1e-6));
        assert!(approx_eq(r.y, 10.0, 1e-6));
        assert!(approx_eq(r.z, 15.0, 1e-6));
    }

    #[test]
    fn test_ease_functions_boundaries() {
        // 所有缓动函数在 t=0 时应为 0，t=1 时应为 1
        let fns: [fn(f32) -> f32; 4] = [
            ease_linear,
            ease_out_cubic,
            ease_in_out_quad,
            ease_out_back,
        ];
        for f in fns {
            assert!(approx_eq(f(0.0), 0.0, 1e-6), "ease fn should return 0 at t=0");
            assert!(approx_eq(f(1.0), 1.0, 1e-6), "ease fn should return 1 at t=1");
        }
    }

    #[test]
    fn test_ease_out_cubic_values() {
        assert!(approx_eq(ease_out_cubic(0.5), 1.0 - 0.5f32.powi(3), 1e-6));
    }

    #[test]
    fn test_ease_in_out_quad_midpoint() {
        // 中点应该是 0.5
        assert!(approx_eq(ease_in_out_quad(0.5), 0.5, 1e-6));
    }

    #[test]
    fn test_clamp01() {
        assert!(approx_eq(clamp01(-1.0), 0.0, 1e-6));
        assert!(approx_eq(clamp01(0.5), 0.5, 1e-6));
        assert!(approx_eq(clamp01(2.0), 1.0, 1e-6));
    }

    #[test]
    fn test_animation_controller() {
        let mut anim = AnimationController::new(2.0, ease_linear);

        // 未开始时进度为 0
        assert!(!anim.playing);
        assert!(approx_eq(anim.progress(0.0), 0.0, 1e-6));
        assert!(!anim.is_done(0.0));

        // 开始后在不同时间点查询
        anim.start(10.0);
        assert!(anim.playing);

        assert!(approx_eq(anim.progress(10.0), 0.0, 1e-6));    // 刚开始
        assert!(approx_eq(anim.progress(11.0), 0.5, 1e-6));    // 进行一半
        assert!(approx_eq(anim.progress(12.0), 1.0, 1e-6));    // 刚好结束
        assert!(approx_eq(anim.progress(13.0), 1.0, 1e-6));    // 已结束，钳位在 1

        assert!(!anim.is_done(10.0));
        assert!(!anim.is_done(11.0));
        assert!(anim.is_done(12.0));
        assert!(anim.is_done(13.0));
    }

    #[test]
    fn test_animation_controller_with_ease() {
        let mut anim = AnimationController::new(1.0, ease_out_cubic);
        anim.start(0.0);

        // 0.5 秒时，线性进度 0.5，经过 ease_out_cubic 后应该更大（缓出开始快）
        let p = anim.progress(0.5);
        assert!(p > 0.5, "ease_out_cubic at t=0.5 should be > 0.5");
        assert!(p < 1.0);
    }

    #[test]
    fn test_animation_controller_zero_duration() {
        let mut anim = AnimationController::new(0.0, ease_linear);
        anim.start(0.0);
        assert!(anim.is_done(0.0));
        assert!(approx_eq(anim.progress(0.0), 1.0, 1e-6));
    }
}
