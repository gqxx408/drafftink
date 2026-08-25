//! 思维导图自包含查看器
//!
//! 提供完整的思维导图编辑体验，可直接嵌入 egui 应用。
//! 封装了数据模型、布局计算、渲染和交互。

use egui::{Color32, Key, Pos2, Rect, Sense, Stroke, Vec2 as EguiVec2};
use std::collections::HashMap;
use uuid::Uuid;

use crate::interaction::{KeyAction, MindMapEvent, MindMapInteraction};
use crate::layout::{create_layout, LayoutStrategy, Vec2};
use crate::persistence;
use crate::render::MindMapRenderer;
use crate::types::{MapType, MindMapDoc, NodePosition};

/// 思维导图自包含查看器
///
/// # 用法
/// ```ignore
/// let mut viewer = MindMapViewer::new();
/// // 在 egui 中:
/// viewer.ui(ctx);
/// ```
pub struct MindMapViewer {
    /// 文档数据
    pub doc: MindMapDoc,
    /// 布局位置缓存
    positions: HashMap<Uuid, Vec2>,
    /// 布局策略
    layout_strategy: Box<dyn LayoutStrategy>,
    /// 交互状态
    pub interaction: MindMapInteraction,
    /// 渲染器
    pub renderer: MindMapRenderer,
    /// 是否需要重新布局
    needs_layout: bool,
    /// 临时文本编辑缓冲
    text_buffer: String,
    /// 当前编辑中的节点
    editing_node: Option<Uuid>,
    /// 上下文菜单目标节点（持久，直到用户操作或关闭）
    context_menu_target: Option<Uuid>,
    /// 缩放级别
    zoom_level: f32,
}

impl Default for MindMapViewer {
    fn default() -> Self {
        Self::new()
    }
}

impl MindMapViewer {
    /// 创建新的思维导图查看器（自动创建空白文档）
    pub fn new() -> Self {
        let doc = MindMapDoc::new("中心主题");
        let layout = create_layout(&doc);
        let positions = layout.layout(&doc, Vec2::new(1200.0, 800.0));

        let interaction = MindMapInteraction {
            selected_node: Some(doc.root_id),
            ..Default::default()
        };

        Self {
            doc,
            positions,
            layout_strategy: layout,
            interaction,
            renderer: MindMapRenderer::default(),
            needs_layout: false,
            text_buffer: String::new(),
            editing_node: None,
            context_menu_target: None,
            zoom_level: 1.0,
        }
    }

    /// 渲染思维导图（全屏模式）
    pub fn ui(&mut self, ctx: &egui::Context) {
        // ── 重新布局（如果需要） ──
        if self.needs_layout {
            let viewport = Vec2::new(ctx.screen_rect().width(), ctx.screen_rect().height());
            self.layout_strategy = create_layout(&self.doc);
            self.positions = self.layout_strategy.layout(&self.doc, viewport);
            self.needs_layout = false;
        }

        let screen_rect = ctx.screen_rect();

        // ── 设置渲染器视口偏移（居中）──
        self.renderer.viewport_offset =
            EguiVec2::new(screen_rect.width() / 2.0, screen_rect.height() / 2.0);
        self.renderer.zoom = self.zoom_level;

        // ── 背景绘制层（Order::Background，仅绘制，不消费交互）──
        let bg_painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("mindmap_bg"),
        ));

        // 纯色背景
        bg_painter.rect_filled(screen_rect, 0.0, Color32::from_rgb(25, 30, 40));

        // 绘制网格背景
        self.draw_grid(&bg_painter, screen_rect);

        // 渲染思维导图（节点 + 连线）
        self.renderer
            .render(&bg_painter, &self.doc, &self.positions, &self.interaction);

        // ── 画布交互层（Order::Middle，低于工具栏的 Foreground）──
        // 工具栏 / 右键菜单在更高层 (Order::Foreground)，会优先获得点击
        egui::Area::new(egui::Id::new("mindmap_canvas"))
            .order(egui::Order::Middle)
            .fixed_pos(egui::pos2(0.0, 0.0))
            .interactable(true)
            .show(ctx, |ui| {
                let response = ui.allocate_rect(screen_rect, Sense::click_and_drag());

                // 悬停检测
                if let Some(mouse_pos) = response.hover_pos() {
                    let hit = self
                        .renderer
                        .hit_test(&self.doc, &self.positions, mouse_pos);
                    if hit != self.interaction.hovered_node {
                        if let Some(id) = hit {
                            let _ = self
                                .interaction
                                .handle_event(MindMapEvent::Hover(id), &mut self.doc);
                        } else {
                            let _ = self
                                .interaction
                                .handle_event(MindMapEvent::Unhover, &mut self.doc);
                        }
                    }
                }

                // 双击编辑
                if response.double_clicked() {
                    if let Some(pos) = response.hover_pos() {
                        if let Some(hit_id) =
                            self.renderer.hit_test(&self.doc, &self.positions, pos)
                        {
                            self.start_edit_node(hit_id);
                        }
                    }
                }

                // 右键菜单
                if response.secondary_clicked() {
                    if let Some(pos) = response.hover_pos() {
                        if let Some(hit_id) =
                            self.renderer.hit_test(&self.doc, &self.positions, pos)
                        {
                            self.interaction.selected_node = Some(hit_id);
                            self.context_menu_target = Some(hit_id);
                        } else {
                            self.context_menu_target = None;
                        }
                    }
                }

                // 左键点击选中
                if response.clicked() {
                    self.context_menu_target = None; // 点击空白处关闭菜单
                    if let Some(pos) = response.hover_pos() {
                        if let Some(hit_id) =
                            self.renderer.hit_test(&self.doc, &self.positions, pos)
                        {
                            let _ = self
                                .interaction
                                .handle_event(MindMapEvent::SelectNode(hit_id), &mut self.doc);
                        } else {
                            let _ = self
                                .interaction
                                .handle_event(MindMapEvent::Deselect, &mut self.doc);
                        }
                    }
                }
            });

        // ── 键盘快捷键 ──
        self.handle_keyboard(ctx);

        // ── 顶部工具栏（Order::Foreground，确保可点击）──
        self.render_toolbar(ctx);

        // ── 文本编辑弹窗 ──
        self.render_text_editor(ctx);

        // ── 右键菜单 ──
        self.render_context_menu(ctx);

        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }

    // ── 工具栏 ──────────────────────────────────────────────────

    fn render_toolbar(&mut self, ctx: &egui::Context) {
        // 提取需要的数据避免借用冲突
        let selected = self.interaction.selected_node;
        let map_type = self.doc.map_type;
        let zoom = self.zoom_level;
        let doc = &self.doc;

        let mut add_child_action = None;
        let mut add_sibling_action = None;
        let mut switch_type_action = None;
        let mut zoom_action = None;
        let mut save_action = false;

        egui::Area::new(egui::Id::new("mindmap_toolbar"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(8.0, 8.0))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(Color32::from_rgba_premultiplied(35, 40, 55, 240))
                    .rounding(egui::Rounding::same(8.0))
                    .stroke(Stroke::new(1.0_f32, Color32::from_gray(80)))
                    .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("🧠 思维导图").size(14.0).strong());
                            ui.separator();

                            // 添加子节点（未选中时默认对根节点操作）
                            if ui.button("＋子节点").clicked() {
                                add_child_action = Some(selected.unwrap_or(doc.root_id));
                            }

                            // 添加兄弟节点
                            let sibling_enabled =
                                selected.is_some() && selected != Some(doc.root_id);
                            ui.add_enabled_ui(sibling_enabled, |ui| {
                                if ui.button("＋兄弟").clicked() {
                                    add_sibling_action = Some(selected.unwrap());
                                }
                            });
                            ui.separator();

                            // 导图类型切换
                            ui.label("类型:");
                            egui::ComboBox::from_id_salt("map_type")
                                .selected_text(map_type_label(map_type))
                                .show_ui(ui, |ui| {
                                    for &(t, label) in &[
                                        (MapType::MindMap, "思维导图"),
                                        (MapType::FishBone, "鱼骨图"),
                                        (MapType::Organization, "组织架构"),
                                        (MapType::Mindly, "星环图"),
                                    ] {
                                        if ui.selectable_label(map_type == t, label).clicked() {
                                            switch_type_action = Some(t);
                                        }
                                    }
                                });

                            ui.separator();

                            // 缩放
                            if ui.button("🔍-").clicked() {
                                zoom_action = Some(-0.1);
                            }
                            ui.label(format!("{:.0}%", zoom * 100.0));
                            if ui.button("🔍+").clicked() {
                                zoom_action = Some(0.1);
                            }

                            ui.separator();

                            // 保存
                            if ui.button("💾 保存").clicked() {
                                save_action = true;
                            }
                        });
                    });
            });

        // ── 处理工具栏按钮动作（在 Area 之外，避免借用冲突）──

        if let Some(parent_id) = add_child_action {
            let position = self
                .doc
                .nodes
                .get(&parent_id)
                .map(|n| n.position)
                .unwrap_or(NodePosition::Right);
            let _ = self
                .interaction
                .handle_event(MindMapEvent::AddChild(parent_id, position), &mut self.doc);
            self.needs_layout = true;
        }

        if let Some(node_id) = add_sibling_action {
            let position = self
                .doc
                .nodes
                .get(&node_id)
                .map(|n| n.position)
                .unwrap_or(NodePosition::Right);
            let _ = self
                .interaction
                .handle_event(MindMapEvent::AddSibling(node_id, position), &mut self.doc);
            self.needs_layout = true;
        }

        if let Some(new_type) = switch_type_action {
            let _ = self
                .interaction
                .handle_event(MindMapEvent::SwitchMapType(new_type), &mut self.doc);
            self.needs_layout = true;
        }

        if let Some(delta) = zoom_action {
            self.zoom_level = (self.zoom_level + delta).clamp(0.3, 3.0);
        }

        if save_action {
            let path = std::path::PathBuf::from("mindmap.ron");
            let _ = persistence::save_to_file(&self.doc, &path);
        }
    }

    // ── 文本编辑 ────────────────────────────────────────────────

    fn render_text_editor(&mut self, ctx: &egui::Context) {
        let Some(node_id) = self.editing_node else {
            return;
        };

        if !self.doc.nodes.contains_key(&node_id) {
            self.editing_node = None;
            return;
        }

        let pos = self.positions.get(&node_id).copied().unwrap_or(Vec2::ZERO);
        let screen_pos = self.renderer.transform_pos(pos);

        egui::Window::new("edit_node_text")
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .fixed_pos(Pos2::new(screen_pos.x - 100.0, screen_pos.y - 20.0))
            .fixed_size(EguiVec2::new(200.0, 40.0))
            .show(ctx, |ui| {
                let resp = ui.add_sized(
                    [ui.available_width(), 20.0],
                    egui::TextEdit::singleline(&mut self.text_buffer)
                        .font(egui::FontId::proportional(14.0)),
                );
                resp.request_focus();
                let enter_pressed = ui.input(|i| i.key_pressed(Key::Enter));
                let escaped = ui.input(|i| i.key_pressed(Key::Escape));
                if resp.lost_focus() || enter_pressed || escaped {
                    if !escaped {
                        let text = self.text_buffer.trim().to_string();
                        if !text.is_empty() {
                            // 需要先把 text_buffer 取出来避免借用冲突
                            let _ = self
                                .interaction
                                .handle_event(MindMapEvent::EditText(node_id, text), &mut self.doc);
                            self.needs_layout = true;
                        }
                    }
                    self.editing_node = None;
                    self.text_buffer.clear();
                }
            });
    }

    // ── 右键菜单（持久显示，直到用户操作或点击其他地方）──

    fn render_context_menu(&mut self, ctx: &egui::Context) {
        let Some(node_id) = self.context_menu_target else {
            return;
        };

        // 检查节点是否还存在
        if !self.doc.nodes.contains_key(&node_id) {
            self.context_menu_target = None;
            return;
        }

        let pos = match self.positions.get(&node_id).copied() {
            Some(p) => p,
            None => {
                self.context_menu_target = None;
                return;
            }
        };
        let screen_pos = self.renderer.transform_pos(pos);
        let is_root = node_id == self.doc.root_id;
        let node_position = self
            .doc
            .nodes
            .get(&node_id)
            .map(|n| n.position)
            .unwrap_or(NodePosition::Right);
        let has_children = self
            .doc
            .nodes
            .get(&node_id)
            .map(|n| !n.children.is_empty())
            .unwrap_or(false);
        let is_collapsed = self
            .doc
            .nodes
            .get(&node_id)
            .map(|n| n.collapsed)
            .unwrap_or(false);

        // 收集动作，在 Area 之外执行
        let mut action = ContextMenuAction::None;

        egui::Area::new(egui::Id::new("mindmap_context_menu"))
            .fixed_pos(Pos2::new(screen_pos.x, screen_pos.y + 20.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(Color32::from_rgba_premultiplied(40, 44, 55, 250))
                    .rounding(egui::Rounding::same(6.0))
                    .stroke(Stroke::new(1.0_f32, Color32::from_gray(90)))
                    .inner_margin(egui::Margin::same(4.0))
                    .show(ui, |ui| {
                        ui.set_min_width(120.0);
                        ui.spacing_mut().button_padding = EguiVec2::new(4.0, 2.0);
                        if ui.button("✏ 编辑文本").clicked() {
                            action = ContextMenuAction::Edit;
                        }
                        if ui.button("＋ 添加子节点").clicked() {
                            action = ContextMenuAction::AddChild;
                        }
                        if !is_root && ui.button("＋ 添加兄弟").clicked() {
                            action = ContextMenuAction::AddSibling;
                        }
                        ui.separator();
                        if !is_root && ui.button("🗑 删除节点").clicked() {
                            action = ContextMenuAction::Delete;
                        }
                        if has_children {
                            ui.separator();
                            let label = if is_collapsed {
                                "▶ 展开"
                            } else {
                                "▼ 收起"
                            };
                            if ui.button(label).clicked() {
                                action = ContextMenuAction::ToggleCollapse;
                            }
                        }
                    });
            });

        // 执行动作
        match action {
            ContextMenuAction::None => {} // 菜单仍然打开
            ContextMenuAction::Edit => {
                self.start_edit_node(node_id);
                self.context_menu_target = None;
            }
            ContextMenuAction::AddChild => {
                let _ = self.interaction.handle_event(
                    MindMapEvent::AddChild(node_id, node_position),
                    &mut self.doc,
                );
                self.needs_layout = true;
                self.context_menu_target = None;
            }
            ContextMenuAction::AddSibling => {
                let _ = self.interaction.handle_event(
                    MindMapEvent::AddSibling(node_id, node_position),
                    &mut self.doc,
                );
                self.needs_layout = true;
                self.context_menu_target = None;
            }
            ContextMenuAction::Delete => {
                let _ = self
                    .interaction
                    .handle_event(MindMapEvent::DeleteNode(node_id), &mut self.doc);
                self.needs_layout = true;
                self.context_menu_target = None;
            }
            ContextMenuAction::ToggleCollapse => {
                let _ = self
                    .interaction
                    .handle_event(MindMapEvent::ToggleCollapse(node_id), &mut self.doc);
                self.needs_layout = true;
                self.context_menu_target = None;
            }
        }
    }

    // ── 键盘处理 ────────────────────────────────────────────────

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        // 文本编辑中不处理快捷键
        if self.editing_node.is_some() {
            return;
        }

        let input = ctx.input(|i| i.clone());

        if input.key_pressed(Key::Tab) {
            let event = MindMapEvent::KeyPress(KeyAction::Tab);
            if self
                .interaction
                .handle_event(event, &mut self.doc)
                .unwrap_or(false)
            {
                self.needs_layout = true;
            }
        }
        if input.key_pressed(Key::Enter) {
            let event = MindMapEvent::KeyPress(KeyAction::Enter);
            if self
                .interaction
                .handle_event(event, &mut self.doc)
                .unwrap_or(false)
            {
                self.needs_layout = true;
            }
        }
        if input.key_pressed(Key::Delete) || input.key_pressed(Key::Backspace) {
            let event = MindMapEvent::KeyPress(KeyAction::Delete);
            if self
                .interaction
                .handle_event(event, &mut self.doc)
                .unwrap_or(false)
            {
                self.needs_layout = true;
            }
        }
        if input.key_pressed(Key::Escape) {
            self.editing_node = None;
            self.text_buffer.clear();
            self.context_menu_target = None;
            let _ = self
                .interaction
                .handle_event(MindMapEvent::KeyPress(KeyAction::Escape), &mut self.doc);
        }
        if input.key_pressed(Key::F2) {
            if let Some(selected) = self.interaction.selected_node {
                self.start_edit_node(selected);
            }
        }
    }

    // ── 辅助方法 ────────────────────────────────────────────────

    fn start_edit_node(&mut self, node_id: Uuid) {
        self.editing_node = Some(node_id);
        self.text_buffer = self
            .doc
            .nodes
            .get(&node_id)
            .map(|n| n.title.to_plain_text())
            .unwrap_or_default();
    }

    /// 绘制网格背景
    fn draw_grid(&self, painter: &egui::Painter, rect: Rect) {
        let grid_size = 40.0 * self.zoom_level;
        let color = Color32::from_rgba_premultiplied(60, 65, 80, 80);

        let start_x = (rect.min.x / grid_size).floor() * grid_size;
        let start_y = (rect.min.y / grid_size).floor() * grid_size;

        let mut x = start_x;
        while x <= rect.max.x {
            painter.line_segment(
                [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
                Stroke::new(0.5_f32, color),
            );
            x += grid_size;
        }

        let mut y = start_y;
        while y <= rect.max.y {
            painter.line_segment(
                [Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)],
                Stroke::new(0.5_f32, color),
            );
            y += grid_size;
        }
    }
}

/// 右键菜单动作
enum ContextMenuAction {
    None,
    Edit,
    AddChild,
    AddSibling,
    Delete,
    ToggleCollapse,
}

fn map_type_label(t: MapType) -> &'static str {
    match t {
        MapType::MindMap => "思维导图",
        MapType::FishBone => "鱼骨图",
        MapType::Organization => "组织架构",
        MapType::Mindly => "星环图",
    }
}
