//! 标签 UI：天体名称、信息标注等
//!
//! 提供 Billboard 方式的 3D 标签渲染，包括文字标签和连接线。

use egui::{Color32, Painter, Pos2, Rect, Stroke};
use nalgebra::{Point3, Vector3};

use crate::ecs::{Label, Transform};
use crate::render::camera::project_point;
use crate::render::SceneRenderer;

/// 渲染所有 3D 标签（Billboard 方式）
///
/// 遍历所有实体，如果有 Label 组件且 visible = true，
/// 则计算世界空间位置并投影到屏幕空间，绘制标签文字和连接线。
///
/// # 参数
/// - `renderer`：场景渲染器（用于获取 VP 矩阵）
/// - `painter`：egui 绘制器
/// - `rect`：渲染区域
/// - `transforms`：所有实体的变换组件数组
/// - `labels`：所有实体的标签组件数组（与 transforms 同长，None 表示无标签）
///
/// # 返回值
/// 返回成功渲染的标签数量。
pub fn render_labels(
    renderer: &SceneRenderer,
    painter: &Painter,
    rect: Rect,
    transforms: &[Transform],
    labels: &[Option<Label>],
) -> usize {
    let vp = renderer.camera().view_projection();
    let screen_width = rect.width();
    let screen_height = rect.height();
    let mut count = 0;

    for i in 0..transforms.len().min(labels.len()) {
        if let Some(label) = &labels[i] {
            if !label.visible {
                continue;
            }

            // 计算实体的世界空间位置
            let entity_pos = Point3::from(transforms[i].position);

            // 计算标签的世界空间位置（实体位置 + 偏移）
            let label_world_pos = Point3::from(transforms[i].position + label.offset);

            // 投影标签位置到屏幕空间
            let label_screen = project_point(&vp, &label_world_pos, screen_width, screen_height);

            // 投影实体位置到屏幕空间（用于绘制连接线）
            let entity_screen = project_point(&vp, &entity_pos, screen_width, screen_height);

            if let (Some(label_p), Some(entity_p)) = (label_screen, entity_screen) {
                let label_pos = Pos2::new(rect.min.x + label_p.x, rect.min.y + label_p.y);
                let entity_pos_2d = Pos2::new(rect.min.x + entity_p.x, rect.min.y + entity_p.y);

                // 绘制连接线（从实体位置到标签文字下方）
                let text_bottom = Pos2::new(label_pos.x, label_pos.y + 6.0);
                let line_color = Color32::from_rgba_unmultiplied(
                    (label.color[0] * 255.0) as u8,
                    (label.color[1] * 255.0) as u8,
                    (label.color[2] * 255.0) as u8,
                    150,
                );
                painter.line_segment(
                    [entity_pos_2d, text_bottom],
                    Stroke::new(1.0, line_color),
                );

                // 绘制标签文字
                let text_color = Color32::from_rgb(
                    (label.color[0] * 255.0) as u8,
                    (label.color[1] * 255.0) as u8,
                    (label.color[2] * 255.0) as u8,
                );

                // 文字背景（半透明黑色圆角矩形，提升可读性）
                let font_id = egui::FontId::proportional(12.0);
                let galley = painter.layout_no_wrap(label.text.clone(), font_id, text_color);
                let text_size = galley.size();

                let bg_min = Pos2::new(
                    label_pos.x - text_size.x / 2.0 - 4.0,
                    label_pos.y - text_size.y / 2.0 - 2.0,
                );
                let bg_max = Pos2::new(
                    label_pos.x + text_size.x / 2.0 + 4.0,
                    label_pos.y + text_size.y / 2.0 + 2.0,
                );
                let bg_rect = Rect::from_min_max(bg_min, bg_max);

                painter.rect_filled(
                    bg_rect,
                    3.0,
                    Color32::from_black_alpha(160),
                );
                painter.rect_stroke(
                    bg_rect,
                    3.0,
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(
                        (label.color[0] * 255.0) as u8,
                        (label.color[1] * 255.0) as u8,
                        (label.color[2] * 255.0) as u8,
                        120,
                    )),
                );

                painter.galley(
                    Pos2::new(label_pos.x - text_size.x / 2.0, label_pos.y - text_size.y / 2.0),
                    galley,
                    Color32::WHITE,
                );

                count += 1;
            }
        }
    }

    count
}

/// 渲染单个 3D 标签（便捷函数）
///
/// 用于需要单独控制标签渲染时机的场景。
/// 绘制标签文字 + 连接线（从标签到实体位置）。
///
/// # 返回值
/// 返回标签是否成功渲染（是否在视野内）。
pub fn render_single_label(
    renderer: &SceneRenderer,
    painter: &Painter,
    rect: Rect,
    entity_position: &Vector3<f32>,
    label: &Label,
) -> bool {
    let vp = renderer.camera().view_projection();
    let screen_width = rect.width();
    let screen_height = rect.height();

    let entity_pos = Point3::from(*entity_position);
    let label_pos = Point3::from(*entity_position + label.offset);

    let label_screen = project_point(&vp, &label_pos, screen_width, screen_height);
    let entity_screen = project_point(&vp, &entity_pos, screen_width, screen_height);

    if let (Some(label_p), Some(entity_p)) = (label_screen, entity_screen) {
        let label_2d = Pos2::new(rect.min.x + label_p.x, rect.min.y + label_p.y);
        let entity_2d = Pos2::new(rect.min.x + entity_p.x, rect.min.y + entity_p.y);

        // 连接线
        let text_bottom = Pos2::new(label_2d.x, label_2d.y + 6.0);
        let line_color = Color32::from_rgba_unmultiplied(
            (label.color[0] * 255.0) as u8,
            (label.color[1] * 255.0) as u8,
            (label.color[2] * 255.0) as u8,
            150,
        );
        painter.line_segment(
            [entity_2d, text_bottom],
            Stroke::new(1.0, line_color),
        );

        // 文字颜色
        let text_color = Color32::from_rgb(
            (label.color[0] * 255.0) as u8,
            (label.color[1] * 255.0) as u8,
            (label.color[2] * 255.0) as u8,
        );

        // 文字布局
        let font_id = egui::FontId::proportional(12.0);
        let galley = painter.layout_no_wrap(label.text.clone(), font_id, text_color);
        let text_size = galley.size();

        // 背景
        let bg_min = Pos2::new(
            label_2d.x - text_size.x / 2.0 - 4.0,
            label_2d.y - text_size.y / 2.0 - 2.0,
        );
        let bg_max = Pos2::new(
            label_2d.x + text_size.x / 2.0 + 4.0,
            label_2d.y + text_size.y / 2.0 + 2.0,
        );
        let bg_rect = Rect::from_min_max(bg_min, bg_max);

        painter.rect_filled(bg_rect, 3.0, Color32::from_black_alpha(160));
        painter.rect_stroke(
            bg_rect,
            3.0,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(
                (label.color[0] * 255.0) as u8,
                (label.color[1] * 255.0) as u8,
                (label.color[2] * 255.0) as u8,
                120,
            )),
        );

        painter.galley(
            Pos2::new(label_2d.x - text_size.x / 2.0, label_2d.y - text_size.y / 2.0),
            galley,
            Color32::WHITE,
        );

        true
    } else {
        false
    }
}
