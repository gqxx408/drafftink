//! 物理编辑器 —— 左侧工具栏 + 中央画布，纯 egui 实现。
//!
//! 功能：
//! - 左侧 SidePanel：5 种物理图元按钮 + 操作说明
//! - 中央 CentralPanel：画布，放置和操作图元
//! - 点击按钮 → 在画布中心生成对应图元
//! - 拖拽移动图元
//! - 点击选中 / 取消选中
//! - Delete 键删除选中的图元
//! - 右上角实时内存占用显示

use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};
use sysinfo::{Pid, System};
use uuid::Uuid;

use crate::physics::elements::{
    draw_element, BatteryData, BulbData, LensData, MirrorData, PhysicsElement, ResistorData,
};

/// 物理编辑器状态。
pub struct PhysicsEditor {
    /// 画布上的所有图元
    elements: Vec<PhysicsElement>,
    /// 正在被拖拽的图元 ID（None 表示没有拖拽）
    dragging_id: Option<Uuid>,
    /// 拖拽开始时鼠标相对于图元左上角的偏移
    drag_offset: Vec2,
    /// 系统信息（用于内存监控）
    system: System,
    /// 上次刷新系统信息的时间（避免每帧都刷新）
    last_sys_refresh: f64,
}

impl Default for PhysicsEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysicsEditor {
    /// 创建一个新的物理编辑器。
    pub fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_all();

        Self {
            elements: Vec::new(),
            dragging_id: None,
            drag_offset: Vec2::ZERO,
            system,
            last_sys_refresh: 0.0,
        }
    }

    /// 渲染整个物理编辑器 UI。
    ///
    /// 布局：
    /// ```text
    /// ┌──────────┬──────────────────────────────┐
    /// │ 工具栏   │  画布（放置物理图元）        │
    /// │          │                              │
    /// │ [电阻]   │                              │
    /// │ [灯泡]   │         ╭───╮                │
    /// │ [电源]   │        │ ●  │               │
    /// │ [透镜]   │         ╰───╯                │
    /// │ [平面镜] │          / \                 │
    /// │          │                              │
    /// └──────────┴──────────────────────────────┘
    /// ```
    pub fn ui(&mut self, ctx: &egui::Context) {
        // ── 第一步：先收集工具栏的"添加图元"命令 ──
        // （不能在 SidePanel 闭包里直接 push 到 self.elements，
        //  因为 egui 的借用规则：&mut Ui 和 &mut self 不能同时存在）
        let mut add_command: Option<PhysicsElement> = None;

        // ── 左侧工具栏 ──
        egui::SidePanel::left("physics_toolbar")
            .default_width(150.0)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    ui.heading("⚡ 物理工具");
                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(8.0);
                });

                ui.vertical(|ui| {
                    ui.add_space(4.0);
                    if ui.add(tool_button("🔌  电阻")).clicked() {
                        add_command = Some(PhysicsElement::Resistor(ResistorData::new(Pos2::new(
                            0.0, 0.0,
                        ))));
                    }
                    if ui.add(tool_button("💡  灯泡")).clicked() {
                        add_command =
                            Some(PhysicsElement::Bulb(BulbData::new(Pos2::new(0.0, 0.0))));
                    }
                    if ui.add(tool_button("🔋  电源")).clicked() {
                        add_command = Some(PhysicsElement::Battery(BatteryData::new(Pos2::new(
                            0.0, 0.0,
                        ))));
                    }
                    if ui.add(tool_button("🔍  透镜")).clicked() {
                        add_command =
                            Some(PhysicsElement::Lens(LensData::new(Pos2::new(0.0, 0.0))));
                    }
                    if ui.add(tool_button("🪞  平面镜")).clicked() {
                        add_command =
                            Some(PhysicsElement::Mirror(MirrorData::new(Pos2::new(0.0, 0.0))));
                    }
                });

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);

                ui.label(
                    egui::RichText::new("操作说明")
                        .small()
                        .strong()
                        .color(Color32::from_rgb(80, 80, 80)),
                );
                ui.add_space(4.0);
                ui.label(egui::RichText::new("• 点击按钮添加图元").small());
                ui.label(egui::RichText::new("• 拖拽移动图元").small());
                ui.label(egui::RichText::new("• 点击选中/取消").small());
                ui.label(egui::RichText::new("• Delete 删除选中").small());
                ui.label(egui::RichText::new("• Shift+点击 多选").small());

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);

                // 图元数量统计
                ui.label(
                    egui::RichText::new(format!("📊 图元: {}", self.elements.len()))
                        .small()
                        .color(Color32::from_rgb(100, 100, 100)),
                );

                // 清空按钮
                ui.add_space(8.0);
                if !self.elements.is_empty()
                    && ui
                        .add(egui::Button::new("🗑️  清空画布").min_size(Vec2::new(130.0, 30.0)))
                        .clicked()
                    {
                        self.elements.clear();
                    }
            });

        // ── 中央画布 ──
        // 先预计算一个中心（如果窗口还没显示，后面会用 CentralPanel 的实际尺寸覆盖）
        let mut canvas_center = ctx.screen_rect().center();
        let mut canvas_rect = ctx.screen_rect();

        egui::CentralPanel::default().show(ctx, |ui| {
            canvas_rect = ui.max_rect();
            canvas_center = canvas_rect.center();
            let painter = ui.painter_at(canvas_rect);

            // 绘制浅灰色网格背景
            self.draw_grid(&painter, canvas_rect);

            // 处理画布交互 + 绘制图元
            self.handle_canvas(ui, canvas_rect, &painter);

            // 右上角内存监控
            self.draw_memory_monitor(ctx, &painter, canvas_rect);
        });

        // ── 第二步：执行"添加图元"命令 ──
        // 放在画布渲染之后，这样我们知道画布中心坐标
        if let Some(elem) = add_command {
            self.add_element_at_center(elem, canvas_center);
        }
    }

    /// 绘制网格背景。
    fn draw_grid(&self, painter: &egui::Painter, rect: Rect) {
        let grid_size = 30.0;
        let grid_color = Color32::from_rgb(240, 240, 240);

        // 垂直线
        let mut x = rect.left();
        while x <= rect.right() {
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(1.0_f32, grid_color),
            );
            x += grid_size;
        }

        // 水平线
        let mut y = rect.top();
        while y <= rect.bottom() {
            painter.line_segment(
                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                Stroke::new(1.0_f32, grid_color),
            );
            y += grid_size;
        }
    }

    /// 处理画布交互：绘制图元、检测点击、处理拖拽。
    fn handle_canvas(&mut self, ui: &mut egui::Ui, canvas_rect: Rect, painter: &egui::Painter) {
        // 让画布区域可以响应鼠标事件
        let response = ui.interact(
            canvas_rect,
            egui::Id::new("physics_canvas"),
            Sense::click_and_drag(),
        );

        let mouse_pos = response
            .hover_pos()
            .or_else(|| response.interact_pointer_pos());

        // ── 处理点击 ──
        if response.clicked() {
            if let Some(pos) = mouse_pos {
                // 检查是否点中了某个图元（从后往前查，后面的在上面）
                let mut hit: Option<Uuid> = None;
                for elem in self.elements.iter().rev() {
                    if elem.contains(pos) {
                        hit = Some(elem.id());
                        break;
                    }
                }

                if let Some(id) = hit {
                    // 点中了图元，切换选中状态
                    let shift_pressed = ui.input(|i| i.modifiers.shift);
                    for elem in &mut self.elements {
                        let is_hit = elem.id() == id;
                        if shift_pressed {
                            if is_hit {
                                let cur = elem.base().selected;
                                elem.set_selected(!cur);
                            }
                        } else {
                            elem.set_selected(is_hit);
                        }
                    }
                } else {
                    // 点空白处，取消所有选中
                    for elem in &mut self.elements {
                        elem.set_selected(false);
                    }
                }
            }
        }

        // ── 处理拖拽开始 ──
        if response.drag_started() {
            if let Some(pos) = mouse_pos {
                // 找到被点中的图元（从后往前，优先最上层的）
                for elem in self.elements.iter().rev() {
                    if elem.contains(pos) {
                        self.dragging_id = Some(elem.id());
                        self.drag_offset = pos - elem.base().position;
                        // 拖拽开始时同时选中
                        let id = elem.id();
                        for e in &mut self.elements {
                            e.set_selected(e.id() == id);
                        }
                        break;
                    }
                }
            }
        }

        // ── 处理拖拽中 ──
        if response.dragged() {
            if let (Some(drag_id), Some(pos)) = (self.dragging_id, mouse_pos) {
                for elem in &mut self.elements {
                    if elem.id() == drag_id {
                        elem.base_mut().position = pos - self.drag_offset;
                    }
                }
            }
        }

        // ── 处理拖拽结束 ──
        if response.drag_stopped() {
            self.dragging_id = None;
        }

        // ── 处理键盘删除 ──
        let delete_pressed =
            ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));
        if delete_pressed {
            self.elements.retain(|e| !e.base().selected);
        }

        // ── 绘制所有图元 ──
        for elem in &self.elements {
            draw_element(painter, elem);
        }
    }

    /// 绘制内存监控（右上角小标签）。
    fn draw_memory_monitor(&mut self, ctx: &egui::Context, painter: &egui::Painter, rect: Rect) {
        // 每 1 秒刷新一次系统信息
        let now = ctx.input(|i| i.time);
        if now - self.last_sys_refresh > 1.0 {
            self.system.refresh_memory();
            self.system.refresh_cpu();
            self.last_sys_refresh = now;
        }

        // 获取当前进程的内存占用
        let pid = Pid::from_u32(std::process::id());
        let mem_mb = self
            .system
            .process(pid)
            .map(|p| (p.memory() as f64) / 1024.0 / 1024.0) // KB → MB (sysinfo 返回 KB)
            .unwrap_or(0.0);

        let elem_count = self.elements.len();

        let label = format!("💾 {:.1} MB  |  🧩 {} 个图元", mem_mb, elem_count);
        let text_color = Color32::from_rgb(80, 80, 80);
        let bg_color = Color32::from_rgba_unmultiplied(255, 255, 255, 220);

        let galley = painter.layout_no_wrap(label, egui::FontId::monospace(12.0), text_color);

        let padding = Vec2::new(10.0, 6.0);
        let label_rect = Rect::from_min_size(
            Pos2::new(
                rect.right() - galley.size().x - padding.x * 2.0 - 8.0,
                rect.top() + 8.0,
            ),
            galley.size() + padding * 2.0,
        );

        // 背景
        painter.rect_filled(label_rect, 6.0, bg_color);
        painter.rect_stroke(
            label_rect,
            6.0,
            Stroke::new(1.0_f32, Color32::from_rgb(200, 200, 200)),
        );

        // 文字
        painter.galley(
            Pos2::new(label_rect.left() + padding.x, label_rect.top() + padding.y),
            galley,
            text_color,
        );
    }

    // ── 公共 API ────────────────────────────────────────────────────────

    /// 添加一个图元到画布中心。
    pub fn add_element_at_center(&mut self, mut elem: PhysicsElement, canvas_center: Pos2) {
        let size = elem.base().size;
        elem.base_mut().position = Pos2::new(
            canvas_center.x - size.x / 2.0,
            canvas_center.y - size.y / 2.0,
        );
        self.elements.push(elem);
    }

    /// 添加一个图元到指定位置。
    #[allow(dead_code)]
    pub fn add_element(&mut self, elem: PhysicsElement) {
        self.elements.push(elem);
    }

    /// 获取所有图元（只读）。
    #[allow(dead_code)]
    pub fn elements(&self) -> &[PhysicsElement] {
        &self.elements
    }

    /// 图元数量。
    #[allow(dead_code)]
    pub fn element_count(&self) -> usize {
        self.elements.len()
    }

    /// 清空画布。
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.elements.clear();
    }
}

// ─── 辅助函数 ──────────────────────────────────────────────────────────────

/// 创建一个统一风格的工具栏按钮。
fn tool_button(text: &str) -> egui::Button<'_> {
    egui::Button::new(text)
        .min_size(Vec2::new(130.0, 36.0))
        .fill(Color32::from_rgb(245, 245, 245))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(200, 200, 200)))
}

// ─── 为了让 LensType 能被 elements 模块外部使用 ──────────────────────────
// （LensData 里用了 LensType，已经在 elements.rs 里定义并导出了）
