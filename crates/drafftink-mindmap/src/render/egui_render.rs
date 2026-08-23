//! egui 2D 思维导图渲染器
//!
//! 将 MindMapDoc + 布局位置渲染为 egui 图形。
//!
//! # 渲染流程
//! 1. 绘制连线（贝塞尔曲线 / 直线 / 弧线，取决于导图类型）
//! 2. 绘制节点（背景、边框、文字）
//! 3. 处理交互（悬停检测、点击检测）

use egui::{Color32, Painter, Pos2, Rect, Rounding, Stroke, Vec2 as EguiVec2};
use std::collections::HashMap;
use uuid::Uuid;

use crate::layout::Vec2;
use crate::types::{MapType, MindMapDoc, MindNode};
use crate::interaction::MindMapInteraction;

/// 思维导图 2D 渲染器
pub struct MindMapRenderer {
    /// 连线颜色
    pub line_color: Color32,
    /// 连线宽度
    pub line_width: f32,
    /// 悬停高亮颜色
    pub hover_color: Color32,
    /// 选中高亮颜色
    pub selection_color: Color32,
    /// 渲染视口偏移
    pub viewport_offset: EguiVec2,
    /// 渲染缩放
    pub zoom: f32,
}

impl Default for MindMapRenderer {
    fn default() -> Self {
        Self {
            line_color: Color32::from_rgb(100, 110, 130),
            line_width: 2.0,
            hover_color: Color32::from_rgba_premultiplied(100, 180, 255, 60),
            selection_color: Color32::from_rgba_premultiplied(46, 134, 222, 100),
            viewport_offset: EguiVec2::ZERO,
            zoom: 1.0,
        }
    }
}

impl MindMapRenderer {
    /// 渲染完整的思维导图
    ///
    /// # 参数
    /// - `painter`: egui 绘制器
    /// - `doc`: 思维导图文档
    /// - `positions`: 布局算法输出的节点位置
    /// - `interaction`: 交互状态（用于高亮/选中）
    pub fn render(
        &self,
        painter: &Painter,
        doc: &MindMapDoc,
        positions: &HashMap<Uuid, Vec2>,
        interaction: &MindMapInteraction,
    ) {
        // 1. 绘制连线
        self.render_lines(painter, doc, positions);

        // 2. 绘制节点
        self.render_nodes(painter, doc, positions, interaction);
    }

    /// 绘制连线
    fn render_lines(
        &self,
        painter: &Painter,
        doc: &MindMapDoc,
        positions: &HashMap<Uuid, Vec2>,
    ) {
        for node in doc.nodes.values() {
            // 跳过隐藏节点
            if node.hidden {
                continue;
            }
            // 跳过不可见节点（父节点被收起时不画连线）
            if !node.is_visible(doc) {
                continue;
            }

            let c_pos = match positions.get(&node.id) {
                Some(p) => self.transform_pos(*p),
                None => continue,
            };

            let parent_id = match node.parent_id {
                Some(pid) => pid,
                None => continue,
            };

            let parent = match doc.nodes.get(&parent_id) {
                Some(p) => p,
                None => continue,
            };

            let p_pos = match positions.get(&parent_id) {
                Some(p) => self.transform_pos(*p),
                None => continue,
            };

            match doc.map_type {
                MapType::MindMap | MapType::Organization => {
                    self.draw_bezier_curve(painter, p_pos, c_pos, parent, node);
                }
                MapType::FishBone => {
                    self.draw_fishbone_line(painter, p_pos, c_pos);
                }
                MapType::Mindly => {
                    self.draw_arc_curve(painter, p_pos, c_pos);
                }
            }
        }
    }

    /// 绘制贝塞尔曲线（用于树形思维导图）
    ///
    /// 从父节点边缘到子节点边缘绘制三次贝塞尔曲线，
    /// 根据子节点在左/右分支自动调整控制点方向。
    fn draw_bezier_curve(
        &self,
        painter: &Painter,
        from: Pos2,
        to: Pos2,
        parent: &MindNode,
        child: &MindNode,
    ) {
        // 计算父节点和子节点的半宽（考虑缩放）
        let parent_half_w = parent.style.size.x / 2.0 * self.zoom;
        let child_half_w = child.style.size.x / 2.0 * self.zoom;

        // 根据子节点位置决定连线起点和终点（从边缘出发）
        let (start, end) = match child.position {
            crate::types::NodePosition::Right | crate::types::NodePosition::Root => {
                // 右分支：从父节点右边缘到子节点左边缘
                (
                    Pos2::new(from.x + parent_half_w, from.y),
                    Pos2::new(to.x - child_half_w, to.y),
                )
            }
            crate::types::NodePosition::Left => {
                // 左分支：从父节点左边缘到子节点右边缘
                (
                    Pos2::new(from.x - parent_half_w, from.y),
                    Pos2::new(to.x + child_half_w, to.y),
                )
            }
        };

        // 控制点偏移量（水平距离的 40%）
        let dx = (end.x - start.x).abs() * 0.4;
        let direction = if end.x >= start.x { 1.0 } else { -1.0 };
        let ctrl1 = Pos2::new(start.x + direction * dx, start.y);
        let ctrl2 = Pos2::new(end.x - direction * dx, end.y);

        self.draw_cubic_bezier(painter, start, ctrl1, ctrl2, end);
    }

    /// 绘制三次贝塞尔曲线
    fn draw_cubic_bezier(
        &self,
        painter: &Painter,
        p0: Pos2,
        p1: Pos2,
        p2: Pos2,
        p3: Pos2,
    ) {
        let steps = 32;
        let mut points = Vec::with_capacity(steps + 1);

        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let t2 = t * t;
            let t3 = t2 * t;
            let one_minus_t = 1.0 - t;
            let one_minus_t2 = one_minus_t * one_minus_t;
            let one_minus_t3 = one_minus_t2 * one_minus_t;

            let x = one_minus_t3 * p0.x
                + 3.0 * one_minus_t2 * t * p1.x
                + 3.0 * one_minus_t * t2 * p2.x
                + t3 * p3.x;
            let y = one_minus_t3 * p0.y
                + 3.0 * one_minus_t2 * t * p1.y
                + 3.0 * one_minus_t * t2 * p2.y
                + t3 * p3.y;

            points.push(Pos2::new(x, y));
        }

        painter.add(egui::Shape::line(
            points,
            Stroke::new(self.line_width, self.line_color),
        ));
    }

    /// 绘制鱼骨图连线（折线）
    fn draw_fishbone_line(&self, painter: &Painter, from: Pos2, to: Pos2) {
        let mid_x = (from.x + to.x) / 2.0;
        let mid = Pos2::new(mid_x, from.y);
        let corner = Pos2::new(mid_x, to.y);

        painter.add(egui::Shape::line(
            vec![from, mid, corner, to],
            Stroke::new(self.line_width, self.line_color),
        ));
    }

    /// 绘制弧线（用于星环图）
    fn draw_arc_curve(&self, painter: &Painter, from: Pos2, to: Pos2) {
        let dx = to.x - from.x;
        let dy = to.y - from.y;
        let dist = (dx * dx + dy * dy).sqrt();

        if dist < 1.0 {
            return;
        }

        let mid = Pos2::new((from.x + to.x) / 2.0, (from.y + to.y) / 2.0);
        let perp = EguiVec2::new(-dy / dist, dx / dist) * dist * 0.3;
        let ctrl = Pos2::new(mid.x + perp.x, mid.y + perp.y);

        let steps = 24;
        let mut points = Vec::with_capacity(steps + 1);

        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let one_minus_t = 1.0 - t;
            let x = one_minus_t * one_minus_t * from.x
                + 2.0 * one_minus_t * t * ctrl.x
                + t * t * to.x;
            let y = one_minus_t * one_minus_t * from.y
                + 2.0 * one_minus_t * t * ctrl.y
                + t * t * to.y;
            points.push(Pos2::new(x, y));
        }

        painter.add(egui::Shape::line(
            points,
            Stroke::new(self.line_width, self.line_color),
        ));
    }

    /// 绘制所有节点
    fn render_nodes(
        &self,
        painter: &Painter,
        doc: &MindMapDoc,
        positions: &HashMap<Uuid, Vec2>,
        interaction: &MindMapInteraction,
    ) {
        for (node_id, pos) in positions {
            let node = match doc.nodes.get(node_id) {
                Some(n) => n,
                None => continue,
            };

            if node.hidden {
                continue;
            }
            if !node.is_visible(doc) {
                continue;
            }

            let screen_pos = self.transform_pos(*pos);
            self.render_single_node(painter, node, screen_pos, interaction);
        }
    }

    /// 渲染单个节点
    fn render_single_node(
        &self,
        painter: &Painter,
        node: &MindNode,
        pos: Pos2,
        interaction: &MindMapInteraction,
    ) {
        let style = &node.style;
        let half_w = style.size.x / 2.0 * self.zoom;
        let half_h = style.size.y / 2.0 * self.zoom;

        let rect = Rect::from_center_size(pos, EguiVec2::new(half_w * 2.0, half_h * 2.0));

        let is_hovered = interaction.hovered_node == Some(node.id);
        let is_selected = interaction.selected_node == Some(node.id);

        // 选中高亮
        if is_selected {
            painter.rect_filled(
                rect.expand(3.0),
                Rounding::same((style.corner_radius + 2.0) * self.zoom),
                self.selection_color,
            );
        }

        // 悬停高亮
        if is_hovered && !is_selected {
            painter.rect_filled(
                rect.expand(2.0),
                Rounding::same((style.corner_radius + 1.0) * self.zoom),
                self.hover_color,
            );
        }

        // 节点背景
        let fill = Color32::from_rgba_premultiplied(
            style.fill_color[0],
            style.fill_color[1],
            style.fill_color[2],
            style.fill_color[3],
        );
        painter.rect_filled(
            rect,
            Rounding::same(style.corner_radius * self.zoom),
            fill,
        );

        // 节点边框
        let border = Color32::from_rgba_premultiplied(
            style.border_color[0],
            style.border_color[1],
            style.border_color[2],
            style.border_color[3],
        );
        painter.rect_stroke(
            rect,
            Rounding::same(style.corner_radius * self.zoom),
            Stroke::new(style.border_width * self.zoom, border),
        );

        // 节点文字
        let text = node.title.to_plain_text();
        let text_color = Color32::from_rgba_premultiplied(
            style.text_color[0],
            style.text_color[1],
            style.text_color[2],
            style.text_color[3],
        );
        let font_id = egui::FontId::proportional(style.font_size * self.zoom);

        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            font_id,
            text_color,
        );

        // 收起图标（如果有子节点且已收起）
        if !node.children.is_empty() && node.collapsed {
            painter.text(
                Pos2::new(rect.right() + 4.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                "+",
                egui::FontId::proportional(12.0 * self.zoom),
                Color32::from_rgb(180, 190, 210),
            );
        }
    }

    /// 将布局坐标转换为屏幕坐标
    pub fn transform_pos(&self, pos: Vec2) -> Pos2 {
        Pos2::new(
            pos.x * self.zoom + self.viewport_offset.x,
            pos.y * self.zoom + self.viewport_offset.y,
        )
    }

    /// 将屏幕坐标转换为布局坐标
    pub fn screen_to_layout(&self, screen_pos: Pos2) -> Vec2 {
        Vec2::new(
            (screen_pos.x - self.viewport_offset.x) / self.zoom,
            (screen_pos.y - self.viewport_offset.y) / self.zoom,
        )
    }

    /// 获取节点的屏幕矩形
    pub fn node_screen_rect(&self, node: &MindNode, pos: Vec2) -> Rect {
        let screen_pos = self.transform_pos(pos);
        let half_w = node.style.size.x / 2.0 * self.zoom;
        let half_h = node.style.size.y / 2.0 * self.zoom;
        Rect::from_center_size(screen_pos, EguiVec2::new(half_w * 2.0, half_h * 2.0))
    }

    /// 检测鼠标悬停的节点
    pub fn hit_test(
        &self,
        doc: &MindMapDoc,
        positions: &HashMap<Uuid, Vec2>,
        mouse_pos: Pos2,
    ) -> Option<Uuid> {
        for (node_id, pos) in positions {
            let node = match doc.nodes.get(node_id) {
                Some(n) => n,
                None => continue,
            };
            if node.hidden || !node.is_visible(doc) {
                continue;
            }
            let rect = self.node_screen_rect(node, *pos);
            if rect.contains(mouse_pos) {
                return Some(*node_id);
            }
        }
        None
    }
}