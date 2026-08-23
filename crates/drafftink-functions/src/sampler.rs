//! 函数采样
//!
//! 根据视口动态调整采样密度，生成世界坐标点列表。
//! 自动检测不连续点（如 1/x 在 x=0 处），将曲线分段。

use crate::expr::CompiledExpr;
use crate::types::{Parameter, Viewport};

/// 世界坐标点 (x, y)
pub type WorldPoint = [f64; 2];

/// 采样结果：一条曲线可能被不连续点分为多段
pub type SampledSegments = Vec<Vec<WorldPoint>>;

/// 采样器配置
pub struct SamplerConfig {
    /// 每像素采样数（默认 2.0，越高曲线越平滑）
    pub samples_per_pixel: f64,
    /// 不连续检测阈值（y 值跳变超过视口高度的多少倍时认为是断点）
    pub discontinuity_threshold: f64,
}

impl Default for SamplerConfig {
    fn default() -> Self {
        Self {
            samples_per_pixel: 2.0,
            discontinuity_threshold: 3.0,
        }
    }
}

/// 对编译后的表达式在视口 X 范围内采样
///
/// # 参数
/// - `expr`: 编译后的表达式
/// - `params`: 参数列表
/// - `viewport`: 当前视口
/// - `screen_width`: 画布像素宽度（用于确定采样密度）
/// - `config`: 采样器配置
///
/// # 返回
/// 采样后的曲线段列表。连续函数返回单段；有不连续点的函数返回多段。
pub fn sample_function(
    expr: &CompiledExpr,
    params: &[Parameter],
    viewport: &Viewport,
    screen_width: f64,
    config: &SamplerConfig,
) -> SampledSegments {
    let pixel_count = screen_width.max(1.0);
    let num_samples = (pixel_count * config.samples_per_pixel) as usize;
    let num_samples = num_samples.clamp(64, 50000);

    let x_min = viewport.x_min;
    let x_max = viewport.x_max;
    let dx = (x_max - x_min) / num_samples as f64;

    let y_range = viewport.height();
    let jump_threshold = y_range * config.discontinuity_threshold;

    let mut segments: SampledSegments = Vec::new();
    let mut current_segment: Vec<WorldPoint> = Vec::with_capacity(num_samples);

    let mut prev_y: Option<f64> = None;

    for i in 0..=num_samples {
        let x = x_min + dx * i as f64;
        let y = expr.evaluate(params, x);

        match y {
            Some(yv) if yv.is_finite() => {
                // 检测不连续点
                if let Some(py) = prev_y {
                    if (yv - py).abs() > jump_threshold {
                        // 断点：结束当前段，开始新段
                        if !current_segment.is_empty() {
                            segments.push(std::mem::take(&mut current_segment));
                        }
                    }
                }
                current_segment.push([x, yv]);
                prev_y = Some(yv);
            }
            _ => {
                // NaN / Inf / None：断点
                if !current_segment.is_empty() {
                    segments.push(std::mem::take(&mut current_segment));
                }
                prev_y = None;
            }
        }
    }

    if !current_segment.is_empty() {
        segments.push(current_segment);
    }

    segments
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Viewport;

    #[test]
    fn test_sample_sine() {
        let expr = CompiledExpr::parse("sin(x)").unwrap();
        let vp = Viewport::default();
        let config = SamplerConfig::default();
        let segments = sample_function(&expr, &[], &vp, 800.0, &config);

        assert_eq!(segments.len(), 1);
        assert!(segments[0].len() > 100);
        // sin(0) = 0 — 在采样点中找到最接近 x=0 的点
        let zero_pt = segments[0]
            .iter()
            .min_by(|a, b| a[0].abs().partial_cmp(&b[0].abs()).unwrap())
            .unwrap();
        assert!(
            zero_pt[1].abs() < 1e-3,
            "sin(0) 应接近 0，实际得到 {}",
            zero_pt[1]
        );
    }

    #[test]
    fn test_sample_reciprocal() {
        // 1/x 在 x=0 处不连续，应产生 2 段
        // 使用较高的不连续阈值，避免 1/x 在接近 0 时的剧烈变化被误判为多个断点
        let expr = CompiledExpr::parse("1/x").unwrap();
        let vp = Viewport::default();
        let config = SamplerConfig {
            discontinuity_threshold: 100.0,
            ..Default::default()
        };
        let segments = sample_function(&expr, &[], &vp, 800.0, &config);

        assert_eq!(segments.len(), 2);
    }
}
