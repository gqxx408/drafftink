//! 2D 地图视图渲染
//!
//! 提供 2D 模式下的太阳系俯视图渲染，以及选中行星时的表面地图投影。
//! 当没有选中行星时，显示太阳系俯视图（从上往下看）；
//! 当选中行星时，显示该行星的等距圆柱投影表面地图。

use egui::{Color32, Painter, Pos2, Rect, Stroke};

use crate::ecs::PlanetInfo;
use crate::projection::equirectangular::lon_lat_to_xy;
use crate::scene::SolarSystemScene;

/// 2D 地图视图渲染
///
/// 在指定矩形区域内绘制 2D 视图：
/// - 未选中行星时：太阳系俯视图（轨道为圆圈，行星为圆点）
/// - 选中行星时：该行星的等距圆柱投影表面地图
///
/// # 参数
/// - `ui`：egui UI 上下文
/// - `rect`：渲染区域
/// - `scene`：太阳系场景数据
/// - `show_labels`：是否显示标签
/// - `selected_planet`：选中的行星索引（None 表示显示太阳系俯视图）
pub fn render_map_view(
    ui: &mut egui::Ui,
    rect: Rect,
    scene: &SolarSystemScene,
    show_labels: bool,
    selected_planet: Option<usize>,
) {
    let painter = ui.painter_at(rect);

    match selected_planet {
        Some(idx) => render_planet_surface(&painter, rect, scene, idx, show_labels),
        None => render_solar_system_top_down(&painter, rect, scene, show_labels),
    }
}

// ---------------------------------------------------------------------------
// 太阳系俯视图
// ---------------------------------------------------------------------------

/// 渲染太阳系俯视图（从上往下看）
///
/// 太阳在中心，行星轨道为同心圆，行星为彩色圆点。
fn render_solar_system_top_down(
    painter: &Painter,
    rect: Rect,
    scene: &SolarSystemScene,
    show_labels: bool,
) {
    // 背景
    painter.rect_filled(rect, 0.0, Color32::from_rgb(5, 10, 25));

    // 绘制星星背景（伪随机分布）
    draw_starfield(painter, rect, 80);

    let center = Pos2::new(rect.center().x, rect.center().y);
    let min_dim = rect.width().min(rect.height());

    // 找到最远轨道半径，用于缩放
    let max_orbit = scene
        .orbits
        .iter()
        .filter_map(|o| o.as_ref().map(|o| o.semi_major_axis))
        .fold(0.0_f32, f32::max);

    // 缩放系数：让最远的轨道占 85% 的空间
    let scale = if max_orbit > 0.0 {
        (min_dim * 0.42) / max_orbit
    } else {
        1.0
    };

    // ---- 绘制轨道 ----
    for orbit in &scene.orbits {
        if let Some(orbit) = orbit {
            let radius = orbit.semi_major_axis * scale;

            // 椭圆轨道（考虑偏心率）
            let a = radius;
            let b = radius * (1.0 - orbit.eccentricity * 0.5); // 简化：用 b 近似

            draw_ellipse(
                painter,
                center,
                a,
                b,
                orbit.ascending_node,
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(100, 120, 160, 120)),
            );
        }
    }

    // ---- 绘制太阳 ----
    let sun_radius = (scene.transforms[0].scale * scale * 0.5).max(4.0);
    // 太阳光晕
    for i in 0..3 {
        let glow_r = sun_radius * (1.0 + i as f32 * 0.6);
        let alpha = 80 - i * 25;
        painter.circle_filled(
            center,
            glow_r,
            Color32::from_rgba_unmultiplied(255, 200, 80, alpha),
        );
    }
    painter.circle_filled(
        center,
        sun_radius,
        Color32::from_rgb(255, 220, 100),
    );

    if show_labels {
        painter.text(
            Pos2::new(center.x, center.y - sun_radius - 10.0),
            egui::Align2::CENTER_BOTTOM,
            "太阳",
            egui::FontId::proportional(11.0),
            Color32::from_rgb(255, 230, 150),
        );
    }

    // ---- 绘制行星 ----
    for i in 0..scene.entity_count() {
        // 跳过太阳（索引 0）和没有行星信息的实体（如土星环）
        if i == 0 || scene.planet_infos[i].is_none() {
            continue;
        }

        let pos = &scene.transforms[i].position;
        let screen_x = center.x + pos.x * scale;
        let screen_y = center.y - pos.z * scale; // 俯视图：z 轴向下翻转
        let planet_pos = Pos2::new(screen_x, screen_y);

        // 行星大小
        let planet_size = (scene.transforms[i].scale * scale * 0.6).max(2.5);

        // 从材质获取颜色（通过 planet_info 不可用，用标签颜色替代）
        let color = if let Some(label) = &scene.labels[i] {
            Color32::from_rgb(
                (label.color[0] * 255.0) as u8,
                (label.color[1] * 255.0) as u8,
                (label.color[2] * 255.0) as u8,
            )
        } else {
            Color32::WHITE
        };

        // 绘制行星
        painter.circle_filled(planet_pos, planet_size, color);
        painter.circle_stroke(
            planet_pos,
            planet_size,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 80)),
        );

        // 绘制标签
        if show_labels {
            if let Some(label) = &scene.labels[i] {
                painter.text(
                    Pos2::new(planet_pos.x, planet_pos.y - planet_size - 6.0),
                    egui::Align2::CENTER_BOTTOM,
                    &label.text,
                    egui::FontId::proportional(10.0),
                    Color32::from_rgb(
                        (label.color[0] * 255.0) as u8,
                        (label.color[1] * 255.0) as u8,
                        (label.color[2] * 255.0) as u8,
                    ),
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 行星表面地图（等距圆柱投影）
// ---------------------------------------------------------------------------

/// 渲染选中行星的表面地图
///
/// 使用等距圆柱投影绘制行星表面，包含赤道、回归线等参考线。
fn render_planet_surface(
    painter: &Painter,
    rect: Rect,
    scene: &SolarSystemScene,
    planet_idx: usize,
    _show_labels: bool,
) {
    // 获取行星信息
    let info = match &scene.planet_infos[planet_idx] {
        Some(info) => info,
        None => return,
    };

    let planet_color = scene.labels[planet_idx]
        .as_ref()
        .map(|l| Color32::from_rgb(
            (l.color[0] * 255.0) as u8,
            (l.color[1] * 255.0) as u8,
            (l.color[2] * 255.0) as u8,
        ))
        .unwrap_or(Color32::from_rgb(100, 150, 200));

    // 渐变背景
    draw_gradient_background(painter, rect, &planet_color);

    // 绘制网格（经纬线）
    draw_grid_lines(painter, rect);

    // 绘制行星名称和信息
    draw_planet_info_header(painter, rect, info, &planet_color);
}

/// 绘制渐变背景（从深蓝到行星色的渐变）
fn draw_gradient_background(painter: &Painter, rect: Rect, planet_color: &Color32) {
    let steps = 20;
    for i in 0..steps {
        let t = i as f32 / steps as f32;
        let y_start = rect.min.y + rect.height() * t;
        let y_end = rect.min.y + rect.height() * (t + 1.0 / steps as f32);

        // 从深蓝渐变到行星色调
        let r = (10.0 + (planet_color.r() as f32 * 0.3) * t) as u8;
        let g = (15.0 + (planet_color.g() as f32 * 0.3) * t) as u8;
        let b = (35.0 + (planet_color.b() as f32 * 0.4) * t) as u8;

        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(rect.min.x, y_start),
                Pos2::new(rect.max.x, y_end),
            ),
            0.0,
            Color32::from_rgb(r, g, b),
        );
    }
}

/// 绘制经纬网格线
fn draw_grid_lines(painter: &Painter, rect: Rect) {
    let width = rect.width();
    let height = rect.height();

    // 经线（每 30° 一条）
    for lon_deg in (-180..=180).step_by(30) {
        let lon = (lon_deg as f32).to_radians();
        let (x, _) = lon_lat_to_xy(lon, 0.0, width, height);
        let px = rect.min.x + x;

        let is_prime = lon_deg == 0;
        let color = if is_prime {
            Color32::from_rgba_unmultiplied(200, 180, 100, 150)
        } else {
            Color32::from_rgba_unmultiplied(80, 100, 140, 80)
        };

        painter.line_segment(
            [Pos2::new(px, rect.min.y), Pos2::new(px, rect.max.y)],
            Stroke::new(if is_prime { 1.5 } else { 1.0 }, color),
        );
    }

    // 纬线
    let lat_lines = [
        (-90.0, "南极", 180, 160, 120),
        (-66.5, "南极圈", 80, 100, 140),
        (-23.5, "南回归线", 100, 120, 160),
        (0.0, "赤道", 200, 180, 100),
        (23.5, "北回归线", 100, 120, 160),
        (66.5, "北极圈", 80, 100, 140),
        (90.0, "北极", 180, 160, 120),
    ];

    for (lat_deg, label, r, g, b) in &lat_lines {
        let lat = *lat_deg * std::f32::consts::PI / 180.0;
        let (_, y) = lon_lat_to_xy(0.0, lat, width, height);
        let py = rect.min.y + y;

        let is_equator = lat_deg.abs() < 0.1;
        let color = Color32::from_rgba_unmultiplied(*r, *g, *b, if is_equator { 180 } else { 100 });

        painter.line_segment(
            [Pos2::new(rect.min.x, py), Pos2::new(rect.max.x, py)],
            Stroke::new(if is_equator { 1.5 } else { 1.0 }, color),
        );

        // 纬线标签
        painter.text(
            Pos2::new(rect.min.x + 6.0, py),
            egui::Align2::LEFT_CENTER,
            *label,
            egui::FontId::proportional(9.0),
            Color32::from_rgba_unmultiplied(*r, *g, *b, 200),
        );
    }
}

/// 绘制行星信息标题
fn draw_planet_info_header(
    painter: &Painter,
    rect: Rect,
    info: &PlanetInfo,
    planet_color: &Color32,
) {
    // 标题背景条
    let header_height = 40.0;
    let header_rect = Rect::from_min_max(
        Pos2::new(rect.min.x, rect.min.y),
        Pos2::new(rect.max.x, rect.min.y + header_height),
    );
    painter.rect_filled(
        header_rect,
        0.0,
        Color32::from_black_alpha(180),
    );
    painter.line_segment(
        [
            Pos2::new(rect.min.x, rect.min.y + header_height),
            Pos2::new(rect.max.x, rect.min.y + header_height),
        ],
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(
            planet_color.r(),
            planet_color.g(),
            planet_color.b(),
            150,
        )),
    );

    // 行星名称
    painter.text(
        Pos2::new(rect.center().x, rect.min.y + 14.0),
        egui::Align2::CENTER_CENTER,
        &info.name,
        egui::FontId::proportional(16.0),
        Color32::from_rgb(
            planet_color.r(),
            planet_color.g(),
            planet_color.b(),
        ),
    );

    // 副标题：直径和质量
    painter.text(
        Pos2::new(rect.center().x, rect.min.y + 30.0),
        egui::Align2::CENTER_CENTER,
        &format!(
            "直径 {:.0} km  |  质量 {:.2e} kg",
            info.diameter_km,
            info.mass_kg
        ),
        egui::FontId::proportional(10.0),
        Color32::from_rgb(180, 190, 210),
    );
}

// ---------------------------------------------------------------------------
// 辅助绘制函数
// ---------------------------------------------------------------------------

/// 绘制椭圆（用多边形近似）
fn draw_ellipse(
    painter: &Painter,
    center: Pos2,
    a: f32,
    b: f32,
    rotation: f32,
    stroke: Stroke,
) {
    let segments = 64;
    let cos_rot = rotation.cos();
    let sin_rot = rotation.sin();

    let mut points = Vec::with_capacity(segments);
    for i in 0..segments {
        let angle = (i as f32 / segments as f32) * std::f32::consts::TAU;
        let x = a * angle.cos();
        let y = b * angle.sin();

        // 应用旋转
        let rx = x * cos_rot - y * sin_rot;
        let ry = x * sin_rot + y * cos_rot;

        points.push(Pos2::new(center.x + rx, center.y + ry));
    }

    // 连接各段
    for i in 0..segments {
        let next = (i + 1) % segments;
        painter.line_segment([points[i], points[next]], stroke);
    }
}

/// 绘制星空背景
fn draw_starfield(painter: &Painter, rect: Rect, count: usize) {
    // 使用简单的伪随机数生成，避免外部依赖
    let mut seed = 42u32;
    let mut rand = || -> f32 {
        // LCG 伪随机数
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed as f32) / (u32::MAX as f32)
    };

    for _ in 0..count {
        let x = rect.min.x + rand() * rect.width();
        let y = rect.min.y + rand() * rect.height();
        let brightness = (80.0 + rand() * 120.0) as u8;
        let size = if rand() > 0.9 { 1.5 } else { 1.0 };

        painter.circle_filled(
            Pos2::new(x, y),
            size,
            Color32::from_rgb(brightness, brightness, brightness + 20),
        );
    }
}
