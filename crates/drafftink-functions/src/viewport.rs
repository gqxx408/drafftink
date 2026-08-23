//! 坐标系变换
//!
//! 实现世界坐标 ↔ 屏幕坐标的双向变换。
//! 世界坐标: 数学坐标系 (x 向右, y 向上)
//! 屏幕坐标: 像素坐标系 (x 向右, y 向下)

use crate::types::Viewport;
use egui::{Pos2, Rect, Vec2};

/// 坐标变换器
///
/// 缓存缩放因子，避免每帧重复计算。
pub struct CoordTransform {
    /// 世界坐标 → 屏幕像素的比例 (pixels per world unit)
    pub scale_x: f64,
    pub scale_y: f64,
    /// 屏幕区域
    pub screen_rect: Rect,
}

impl CoordTransform {
    /// 根据视口和屏幕尺寸创建变换器
    pub fn new(viewport: &Viewport, screen_rect: Rect) -> Self {
        let vw = viewport.width().max(1e-10);
        let vh = viewport.height().max(1e-10);
        let sw = screen_rect.width().max(1.0) as f64;
        let sh = screen_rect.height().max(1.0) as f64;
        Self {
            scale_x: sw / vw,
            scale_y: sh / vh,
            screen_rect,
        }
    }

    /// 世界坐标 → 屏幕坐标
    pub fn world_to_screen(&self, viewport: &Viewport, world_x: f64, world_y: f64) -> Pos2 {
        let sx = self.screen_rect.min.x as f64
            + (world_x - viewport.x_min) * self.scale_x;
        // Y 轴翻转: 世界坐标 y 向上, 屏幕坐标 y 向下
        let sy = self.screen_rect.min.y as f64
            + (viewport.y_max - world_y) * self.scale_y;
        Pos2::new(sx as f32, sy as f32)
    }

    /// 屏幕坐标 → 世界坐标
    pub fn screen_to_world(&self, viewport: &Viewport, screen: Pos2) -> (f64, f64) {
        let wx = viewport.x_min + (screen.x as f64 - self.screen_rect.min.x as f64) / self.scale_x;
        let wy = viewport.y_max - (screen.y as f64 - self.screen_rect.min.y as f64) / self.scale_y;
        (wx, wy)
    }

    /// 世界坐标位移 → 屏幕像素位移
    pub fn world_delta_to_screen(&self, dx: f64, dy: f64) -> Vec2 {
        Vec2::new(
            (dx * self.scale_x) as f32,
            (-dy * self.scale_y) as f32, // Y 翻转
        )
    }

    /// 屏幕像素位移 → 世界坐标位移
    pub fn screen_delta_to_world(&self, delta: Vec2) -> (f64, f64) {
        (
            delta.x as f64 / self.scale_x,
            -delta.y as f64 / self.scale_y, // Y 翻转
        )
    }
}

/// 计算 "漂亮" 的网格间隔 (1, 2, 5, 10, 20, 50, ...)
///
/// 根据视口范围和目标像素间距，选择合适的网格刻度。
pub fn nice_grid_interval(world_range: f64, target_pixels: f64, scale: f64) -> f64 {
    let target_world = world_range * target_pixels / (world_range * scale).max(1.0);
    let target_world = if target_world <= 0.0 { 1.0 } else { target_world };

    let magnitude = 10f64.powi(target_world.log10().floor() as i32);
    let normalized = target_world / magnitude;

    let nice = if normalized < 1.5 {
        1.0
    } else if normalized < 3.5 {
        2.0
    } else if normalized < 7.5 {
        5.0
    } else {
        10.0
    };

    nice * magnitude
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_world_to_screen() {
        let vp = Viewport::default();
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let t = CoordTransform::new(&vp, rect);

        // 原点应该在屏幕中心偏左下
        let origin = t.world_to_screen(&vp, 0.0, 0.0);
        assert!((origin.x - 400.0).abs() < 0.5);
        assert!((origin.y - 300.0).abs() < 0.5);
    }

    #[test]
    fn test_screen_to_world() {
        let vp = Viewport::default();
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let t = CoordTransform::new(&vp, rect);

        let (wx, wy) = t.screen_to_world(&vp, Pos2::new(400.0, 300.0));
        assert!((wx - 0.0).abs() < 0.01);
        assert!((wy - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_nice_interval() {
        // target_world = 20*80/(20*40) = 2.0 → nice=2.0, magnitude=1 → result=2.0
        assert!((nice_grid_interval(20.0, 80.0, 40.0) - 2.0).abs() < 0.01);
        // target_world = 200*80/(200*4) = 20.0 → nice=2.0, magnitude=10 → result=20.0
        assert!((nice_grid_interval(200.0, 80.0, 4.0) - 20.0).abs() < 0.01);
    }
}
