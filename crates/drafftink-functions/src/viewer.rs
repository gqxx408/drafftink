//! 函数绘图查看器
//!
//! 自包含的 egui 组件，提供完整的函数绘图交互体验。
//! 左侧函数列表面板 + 右侧画布 + 编辑弹窗。
//!
//! ## 缓存架构（UUID-keyed）
//!
//! 每条曲线的编译结果、采样数据、错误信息和脏标记都存储在
//! `HashMap<Uuid, CurveCache>` 中，以曲线的唯一 ID 为键。
//! 删除操作仅移除对应 UUID 的条目，绝不影响其他曲线的缓存数据，
//! 彻底避免了索引偏移导致的渲染错位问题。

use std::collections::HashMap;

use crate::expr::CompiledExpr;
use crate::renderer::CurveRenderer;
use crate::sampler::{sample_function, SampledSegments, SamplerConfig};
use crate::types::*;
use crate::viewport::CoordTransform;

use egui::{Color32, Pos2, Sense, Stroke, Vec2};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// CurveCache — 单条曲线的全部缓存数据（UUID-keyed）
// ---------------------------------------------------------------------------

/// 单条曲线的运行时缓存。
///
/// 所有缓存字段以曲线的 UUID 为键存储在 HashMap 中，
/// 删除一条曲线仅移除其对应的缓存条目，不影响其他曲线。
#[derive(Default)]
struct CurveCache {
    /// 编译后的表达式（None 表示编译失败或尚未编译）
    compiled: Option<CompiledExpr>,
    /// 采样后的曲线段（None 表示尚未采样）
    samples: Option<SampledSegments>,
    /// 编译/求值错误信息
    error: Option<String>,
    /// 是否需要重新采样
    dirty: bool,
}

// ---------------------------------------------------------------------------
// EditState — 编辑弹窗状态（UUID-keyed）
// ---------------------------------------------------------------------------

/// 编辑弹窗状态
struct EditState {
    open: bool,
    /// None = 新建, Some(id) = 编辑指定 UUID 的曲线
    curve_id: Option<Uuid>,
    expr_buffer: String,
    color: [u8; 4],
    params: Vec<Parameter>,
    error: Option<String>,
    new_param_name: String,
}

impl Default for EditState {
    fn default() -> Self {
        Self {
            open: false,
            curve_id: None,
            expr_buffer: String::new(),
            color: CURVE_PALETTE[0],
            params: Vec::new(),
            error: None,
            new_param_name: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// FunctionViewer
// ---------------------------------------------------------------------------

/// 函数绘图查看器
pub struct FunctionViewer {
    /// 函数曲线列表
    curves: Vec<FunctionCurve>,
    /// 视口（世界坐标范围）
    viewport: Viewport,
    /// 上一帧的 X 范围（用于检测是否需要重采样）
    prev_x_range: (f64, f64),

    // ── 缓存（UUID-keyed，不序列化）──
    /// 每条曲线的缓存，以 UUID 为键
    cache: HashMap<Uuid, CurveCache>,

    // ── 渲染器 ──
    renderer: CurveRenderer,
    sampler_config: SamplerConfig,

    // ── UI 状态 ──
    edit_state: EditState,
}

impl Default for FunctionViewer {
    fn default() -> Self {
        Self::new()
    }
}

impl FunctionViewer {
    /// 创建新的查看器，自带几条示例曲线
    pub fn new() -> Self {
        let mut viewer = Self {
            curves: Vec::new(),
            viewport: Viewport::default(),
            prev_x_range: (0.0, 0.0),
            cache: HashMap::new(),
            renderer: CurveRenderer::default(),
            sampler_config: SamplerConfig::default(),
            edit_state: EditState::default(),
        };

        // 默认示例曲线
        viewer.add_curve("sin(x)", palette_color(0));
        viewer.add_curve("cos(x)", palette_color(1));
        viewer.add_curve("x^2 / 10", palette_color(2));

        viewer
    }

    /// 主渲染入口
    pub fn ui(&mut self, ctx: &egui::Context) {
        // ── 左侧函数列表面板 ──
        self.render_side_panel(ctx);

        // ── 中央画布 ──
        self.render_canvas(ctx);

        // ── 编辑弹窗 ──
        self.render_edit_dialog(ctx);

        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }

    // ── 左侧面板 ──────────────────────────────────────────────

    fn render_side_panel(&mut self, ctx: &egui::Context) {
        // 收集 UI 动作（在闭包外执行，避免借用冲突）
        let mut add_action = false;
        let mut reset_view = false;
        let mut toggle_id: Option<Uuid> = None;
        let mut edit_id: Option<Uuid> = None;
        let mut delete_id: Option<Uuid> = None;
        let mut param_changed_id: Option<Uuid> = None;

        egui::SidePanel::left("func_panel")
            .resizable(true)
            .default_width(300.0)
            .min_width(220.0)
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(30, 35, 45))
                    .inner_margin(egui::Margin::same(8.0)),
            )
            .show(ctx, |ui| {
                ui.heading(
                    egui::RichText::new("📊 函数绘图")
                        .size(16.0)
                        .strong()
                        .color(Color32::from_rgb(200, 210, 230)),
                );
                ui.add_space(4.0);

                // 添加 / 重置按钮
                ui.horizontal(|ui| {
                    if ui.button("＋ 添加函数").clicked() {
                        add_action = true;
                    }
                    if ui.button("🎯 重置视图").clicked() {
                        reset_view = true;
                    }
                });
                ui.separator();

                // 函数列表 — 按 UUID 标识，不依赖索引
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for curve in &mut self.curves {
                        let curve_id = curve.id;

                        // 从缓存中提取错误信息（clone 避免借用冲突）
                        let error_msg = self.cache.get(&curve_id).and_then(|c| c.error.clone());

                        let color = Color32::from_rgba_unmultiplied(
                            curve.color[0],
                            curve.color[1],
                            curve.color[2],
                            curve.color[3],
                        );

                        egui::Frame::none()
                            .fill(Color32::from_rgba_premultiplied(40, 45, 60, 200))
                            .rounding(egui::Rounding::same(6.0))
                            .inner_margin(egui::Margin::same(6.0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // 颜色色块
                                    let (rect, _) = ui
                                        .allocate_exact_size(Vec2::new(14.0, 14.0), Sense::hover());
                                    ui.painter().rect_filled(rect, 3.0, color);

                                    // 表达式
                                    let expr_text = if curve.visible {
                                        curve.expression.clone()
                                    } else {
                                        format!("({})", curve.expression)
                                    };
                                    ui.label(
                                        egui::RichText::new(&expr_text)
                                            .color(if curve.visible {
                                                Color32::from_rgb(220, 225, 240)
                                            } else {
                                                Color32::from_rgb(100, 105, 120)
                                            })
                                            .size(13.0),
                                    );

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.button("🗑").clicked() {
                                                delete_id = Some(curve_id);
                                            }
                                            if ui.button("✏").clicked() {
                                                edit_id = Some(curve_id);
                                            }
                                            let vis_label =
                                                if curve.visible { "👁" } else { "—" };
                                            if ui.button(vis_label).clicked() {
                                                toggle_id = Some(curve_id);
                                            }
                                        },
                                    );
                                });

                                // 参数滑块
                                if !curve.parameters.is_empty() && curve.visible {
                                    ui.add_space(2.0);
                                    for param in &mut curve.parameters {
                                        let old_val = param.value;
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new(&param.name)
                                                    .size(11.0)
                                                    .color(Color32::from_rgb(150, 160, 180)),
                                            );
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut param.value,
                                                    param.min..=param.max,
                                                )
                                                .step_by(param.step)
                                                .clamping(egui::SliderClamping::Always)
                                                .text(""),
                                            );
                                        });
                                        if param.value != old_val {
                                            param_changed_id = Some(curve_id);
                                        }
                                    }
                                }

                                // 错误提示
                                if let Some(ref err) = error_msg {
                                    ui.add_space(2.0);
                                    ui.colored_label(
                                        Color32::from_rgb(255, 120, 100),
                                        egui::RichText::new(err).size(11.0),
                                    );
                                }
                            });

                        ui.add_space(4.0);
                    }
                });
            });

        // ── 处理动作 ──
        if add_action {
            self.edit_state.open = true;
            self.edit_state.curve_id = None;
            self.edit_state.expr_buffer.clear();
            self.edit_state.color = palette_color(self.curves.len());
            self.edit_state.params.clear();
            self.edit_state.error = None;
            self.edit_state.new_param_name.clear();
        }

        if reset_view {
            self.viewport.reset();
        }

        if let Some(id) = toggle_id {
            if let Some(curve) = self.curves.iter_mut().find(|c| c.id == id) {
                curve.visible = !curve.visible;
            }
        }

        if let Some(id) = edit_id {
            if let Some(curve) = self.curves.iter().find(|c| c.id == id) {
                self.edit_state.open = true;
                self.edit_state.curve_id = Some(id);
                self.edit_state.expr_buffer = curve.expression.clone();
                self.edit_state.color = curve.color;
                self.edit_state.params = curve.parameters.clone();
                self.edit_state.error = None;
                self.edit_state.new_param_name.clear();
            }
        }

        if let Some(id) = delete_id {
            self.remove_curve_by_id(id);
        }

        if let Some(id) = param_changed_id {
            if let Some(entry) = self.cache.get_mut(&id) {
                entry.dirty = true;
            }
        }
    }

    // ── 中央画布 ──────────────────────────────────────────────

    fn render_canvas(&mut self, ctx: &egui::Context) {
        let panel_bg = Color32::from_rgb(24, 28, 38);

        let canvas_rect = egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(panel_bg))
            .show(ctx, |ui| ui.max_rect())
            .inner;

        let transform = CoordTransform::new(&self.viewport, canvas_rect);

        // ── 检查是否需要重采样（X 范围变化）──
        let current_x_range = (self.viewport.x_min, self.viewport.x_max);
        if current_x_range != self.prev_x_range {
            for entry in self.cache.values_mut() {
                entry.dirty = true;
            }
            self.prev_x_range = current_x_range;
        }

        // ── 确保所有曲线都有缓存条目 ──
        self.ensure_cache_entries();

        // ── 重采样（如有需要）──
        // 渲染阶段严格按 UUID 匹配缓存数据，不依赖列表索引
        let screen_w = canvas_rect.width() as f64;
        for curve in &self.curves {
            let curve_id = curve.id;

            // 检查是否需要重采样
            let needs_resample = self.cache.get(&curve_id).is_some_and(|e| e.dirty);

            if !needs_resample {
                continue;
            }

            // 克隆参数以避免借用冲突
            let params = curve.parameters.clone();

            // 在同一个借用作用域内完成采样，按 UUID 定位缓存条目
            if let Some(entry) = self.cache.get_mut(&curve_id) {
                if let Some(ref expr) = entry.compiled {
                    let segments = sample_function(
                        expr,
                        &params,
                        &self.viewport,
                        screen_w,
                        &self.sampler_config,
                    );
                    entry.samples = Some(segments);
                }
                entry.dirty = false;
            }
        }

        // ── 绘制 ──
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("func_canvas"),
        ));

        // 背景
        self.renderer.draw_background(&painter, canvas_rect);

        // 网格
        self.renderer
            .draw_grid(&painter, &self.viewport, &transform, canvas_rect);

        // 曲线 — 按 UUID 匹配缓存数据
        for curve in &self.curves {
            if !curve.visible {
                continue;
            }

            let curve_id = curve.id;
            let entry = match self.cache.get(&curve_id) {
                Some(e) => e,
                None => continue,
            };

            if let Some(ref segments) = entry.samples {
                let color = Color32::from_rgba_unmultiplied(
                    curve.color[0],
                    curve.color[1],
                    curve.color[2],
                    curve.color[3],
                );
                self.renderer
                    .draw_curve(&painter, segments, color, &self.viewport, &transform);

                // 标签
                self.renderer.draw_curve_label(
                    &painter,
                    &curve.expression,
                    color,
                    &self.viewport,
                    &transform,
                    segments,
                );
            }
        }

        // ── 交互：平移 ──
        let drag_delta = ctx.input(|i| i.pointer.delta());
        let primary_down = ctx.input(|i| i.pointer.primary_down());
        let pointer_pos = ctx.input(|i| i.pointer.interact_pos());

        if primary_down && drag_delta != Vec2::ZERO {
            if let Some(_pos) = pointer_pos {
                let (dx, dy) = transform.screen_delta_to_world(drag_delta);
                self.viewport.pan(dx, dy);
            }
        }

        // ── 交互：缩放 ──
        let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.5 {
            if let Some(pos) = pointer_pos {
                // 确保光标在画布内
                if canvas_rect.contains(pos) {
                    let (wx, wy) = transform.screen_to_world(&self.viewport, pos);
                    let factor = if scroll > 0.0 { 0.9 } else { 1.1 };
                    self.viewport.zoom_at(wx, wy, factor);
                }
            }
        }

        // ── 交互：双击添加函数 ──
        let double_click = ctx.input(|i| {
            i.pointer
                .button_double_clicked(egui::PointerButton::Primary)
        });
        if double_click {
            if let Some(pos) = pointer_pos {
                if canvas_rect.contains(pos) {
                    self.edit_state.open = true;
                    self.edit_state.curve_id = None;
                    self.edit_state.expr_buffer.clear();
                    self.edit_state.color = palette_color(self.curves.len());
                    self.edit_state.params.clear();
                    self.edit_state.error = None;
                    self.edit_state.new_param_name.clear();
                }
            }
        }

        // ── 右下角坐标提示 ──
        if let Some(pos) = pointer_pos {
            if canvas_rect.contains(pos) {
                let (wx, wy) = transform.screen_to_world(&self.viewport, pos);
                let hint = format!("({wx:.2}, {wy:.2})");
                painter.text(
                    Pos2::new(canvas_rect.max.x - 8.0, canvas_rect.max.y - 8.0),
                    egui::Align2::RIGHT_BOTTOM,
                    hint,
                    egui::FontId::proportional(12.0),
                    Color32::from_rgb(140, 150, 170),
                );
            }
        }
    }

    // ── 编辑弹窗 ──────────────────────────────────────────────

    fn render_edit_dialog(&mut self, ctx: &egui::Context) {
        if !self.edit_state.open {
            return;
        }

        let mut should_apply = false;
        let mut should_cancel = false;
        let mut should_add_param = false;
        let mut param_to_remove: Option<usize> = None;

        let title = match self.edit_state.curve_id {
            Some(_) => "编辑函数",
            None => "添加函数",
        };

        egui::Window::new(title)
            .fixed_size(Vec2::new(420.0, 400.0))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                // 表达式输入
                ui.label("表达式 f(x) =");
                ui.add(
                    egui::TextEdit::singleline(&mut self.edit_state.expr_buffer)
                        .hint_text("例如: sin(x) * x, a * cos(x), x^2 - 1")
                        .desired_width(380.0),
                );
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new("支持: sin, cos, tan, log, ln, sqrt, abs, pi, e")
                        .size(10.0)
                        .color(Color32::from_rgb(120, 130, 150)),
                );

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // 颜色选择
                ui.label("颜色:");
                ui.horizontal(|ui| {
                    for &c in CURVE_PALETTE {
                        let color = Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
                        let selected = self.edit_state.color == c;
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::new(24.0, 24.0), Sense::click());
                        if response.clicked() {
                            self.edit_state.color = c;
                        }
                        ui.painter().rect_filled(rect, 4.0, color);
                        if selected {
                            ui.painter().rect_stroke(
                                rect,
                                4.0,
                                Stroke::new(2.5_f32, Color32::WHITE),
                            );
                        }
                    }
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // 参数
                ui.label("参数 (可选，用于 f(x, a, b, ...)):");

                for (i, param) in self.edit_state.params.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(&param.name)
                                .strong()
                                .color(Color32::from_rgb(180, 190, 210)),
                        );
                        ui.add(
                            egui::Slider::new(&mut param.value, param.min..=param.max)
                                .step_by(param.step)
                                .clamping(egui::SliderClamping::Always),
                        );
                        ui.label(
                            egui::RichText::new(format!("= {:.2}", param.value))
                                .size(11.0)
                                .color(Color32::from_rgb(150, 160, 180)),
                        );
                        if ui.button("🗑").clicked() {
                            param_to_remove = Some(i);
                        }
                    });
                }

                // 添加参数行
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.edit_state.new_param_name)
                            .hint_text("参数名 (如 a, b, k)")
                            .desired_width(120.0),
                    );
                    if ui.button("＋ 添加参数").clicked() {
                        should_add_param = true;
                    }
                });

                ui.add_space(8.0);

                // 错误提示
                if let Some(ref err) = self.edit_state.error {
                    egui::Frame::none()
                        .fill(Color32::from_rgba_premultiplied(80, 30, 30, 200))
                        .rounding(egui::Rounding::same(4.0))
                        .inner_margin(egui::Margin::same(6.0))
                        .show(ui, |ui| {
                            ui.colored_label(
                                Color32::from_rgb(255, 150, 130),
                                egui::RichText::new(err).size(12.0),
                            );
                        });
                }

                ui.add_space(8.0);

                // 按钮
                ui.horizontal(|ui| {
                    if ui.button("✓ 确定").clicked() {
                        should_apply = true;
                    }
                    if ui.button("✗ 取消").clicked() {
                        should_cancel = true;
                    }
                });
            });

        // ── 处理动作 ──
        if should_add_param {
            let name = if self.edit_state.new_param_name.trim().is_empty() {
                format!("p{}", self.edit_state.params.len() + 1)
            } else {
                self.edit_state.new_param_name.trim().to_string()
            };
            self.edit_state
                .params
                .push(Parameter::new(&name, 1.0, 0.0, 10.0));
            self.edit_state.new_param_name.clear();
        }

        if let Some(idx) = param_to_remove {
            if idx < self.edit_state.params.len() {
                self.edit_state.params.remove(idx);
            }
        }

        if should_cancel {
            self.edit_state.open = false;
            self.edit_state.error = None;
        }

        if should_apply {
            let expr_str = self.edit_state.expr_buffer.trim().to_string();
            if expr_str.is_empty() {
                self.edit_state.error = Some("表达式不能为空".to_string());
            } else {
                match CompiledExpr::parse(&expr_str) {
                    Ok(_) => {
                        match self.edit_state.curve_id {
                            Some(id) => {
                                // 编辑现有曲线 — 按 UUID 查找
                                if let Some(curve) = self.curves.iter_mut().find(|c| c.id == id) {
                                    curve.expression = expr_str;
                                    curve.color = self.edit_state.color;
                                    curve.parameters = self.edit_state.params.clone();
                                    self.recompile_by_id(id);
                                    if let Some(entry) = self.cache.get_mut(&id) {
                                        entry.dirty = true;
                                    }
                                }
                            }
                            None => {
                                // 新建曲线
                                self.add_curve(&expr_str, self.edit_state.color);
                                if let Some(last) = self.curves.last_mut() {
                                    last.parameters = self.edit_state.params.clone();
                                    let new_id = last.id;
                                    self.recompile_by_id(new_id);
                                    if let Some(entry) = self.cache.get_mut(&new_id) {
                                        entry.dirty = true;
                                    }
                                }
                            }
                        }
                        self.edit_state.open = false;
                        self.edit_state.error = None;
                    }
                    Err(e) => {
                        self.edit_state.error = Some(e.to_string());
                    }
                }
            }
        }
    }

    // ── 缓存管理（UUID-keyed）──────────────────────────────────

    /// 确保每条曲线都有对应的缓存条目。
    /// 新曲线会获得一个 dirty=true 的空缓存。
    fn ensure_cache_entries(&mut self) {
        for curve in &self.curves {
            self.cache.entry(curve.id).or_default();
        }
    }

    /// 按 UUID 重新编译曲线的表达式。
    fn recompile_by_id(&mut self, id: Uuid) {
        let expr_str = match self.curves.iter().find(|c| c.id == id) {
            Some(curve) => curve.expression.clone(),
            None => return,
        };

        let entry = self.cache.entry(id).or_default();
        match CompiledExpr::parse(&expr_str) {
            Ok(compiled) => {
                entry.compiled = Some(compiled);
                entry.error = None;
            }
            Err(e) => {
                entry.compiled = None;
                entry.error = Some(e.to_string());
            }
        }
    }

    /// 添加曲线
    fn add_curve(&mut self, expr: &str, color: [u8; 4]) {
        let curve = FunctionCurve::new(expr, color);
        let id = curve.id;
        self.curves.push(curve);
        self.ensure_cache_entries();
        self.recompile_by_id(id);
    }

    /// 按 UUID 删除曲线 — 仅移除目标条目，不影响其他曲线的缓存或位置。
    fn remove_curve_by_id(&mut self, id: Uuid) {
        // 从曲线列表中移除（保留其他曲线的相对顺序）
        if let Some(pos) = self.curves.iter().position(|c| c.id == id) {
            self.curves.remove(pos);
        }
        // 从缓存中移除对应条目
        self.cache.remove(&id);

        // 强制校验缓存一致性
        self.validate_cache();
    }

    /// 校验缓存一致性：移除所有不存在于 curves 中的 UUID 缓存（残留清理），
    /// 并确保所有存活曲线都有缓存条目。
    ///
    /// 每次删除操作后调用，避免缓存残留导致渲染错位。
    fn validate_cache(&mut self) {
        // 收集存活曲线的 UUID 集合
        let alive_ids: std::collections::HashSet<Uuid> = self.curves.iter().map(|c| c.id).collect();

        // 移除所有已不存在的 UUID 缓存（清理残留）
        self.cache.retain(|id, _| alive_ids.contains(id));

        // 确保所有存活曲线都有缓存条目
        for curve in &self.curves {
            self.cache.entry(curve.id).or_default();
        }

        log::debug!(
            "[functions] Cache validated: {} curves, {} cache entries",
            self.curves.len(),
            self.cache.len()
        );
    }
}

// ---------------------------------------------------------------------------
// 单元测试 — UUID-keyed 缓存一致性验证
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 辅助：创建一个带指定表达式的查看器（不带默认曲线）
    fn make_viewer() -> FunctionViewer {
        FunctionViewer {
            curves: Vec::new(),
            viewport: Viewport::default(),
            prev_x_range: (0.0, 0.0),
            cache: HashMap::new(),
            renderer: CurveRenderer::default(),
            sampler_config: SamplerConfig::default(),
            edit_state: EditState::default(),
        }
    }

    /// 每条曲线应获得全局唯一的 UUID。
    #[test]
    fn test_unique_uuids() {
        let mut viewer = make_viewer();
        viewer.add_curve("sin(x)", palette_color(0));
        viewer.add_curve("cos(x)", palette_color(1));
        viewer.add_curve("x^2", palette_color(2));

        let ids: Vec<Uuid> = viewer.curves.iter().map(|c| c.id).collect();
        let unique: std::collections::HashSet<Uuid> = ids.iter().copied().collect();
        assert_eq!(ids.len(), 3);
        assert_eq!(unique.len(), 3, "UUID 必须全局唯一");
    }

    /// 每条曲线添加后都应有对应的缓存条目。
    #[test]
    fn test_cache_entries_created() {
        let mut viewer = make_viewer();
        viewer.add_curve("sin(x)", palette_color(0));
        viewer.add_curve("cos(x)", palette_color(1));

        assert_eq!(viewer.curves.len(), 2);
        assert_eq!(viewer.cache.len(), 2, "缓存条目数应等于曲线数");

        for curve in &viewer.curves {
            assert!(
                viewer.cache.contains_key(&curve.id),
                "每条曲线的 UUID 都应有对应缓存条目"
            );
        }
    }

    /// 【核心】删除曲线后，剩余曲线的缓存数据必须保持不变。
    ///
    /// 这正是原始 Bug 的场景：删除索引 0 的曲线后，
    /// 索引 1 的曲线不应错误映射到被删除曲线的缓存数据。
    #[test]
    fn test_delete_preserves_remaining_cache() {
        let mut viewer = make_viewer();
        viewer.add_curve("sin(x)", palette_color(0));
        viewer.add_curve("cos(x)", palette_color(1));
        viewer.add_curve("x^2", palette_color(2));

        // 记录删除前的缓存状态
        let survivor_a_id = viewer.curves[1].id;
        let survivor_b_id = viewer.curves[2].id;

        // 确认缓存中有编译好的表达式
        assert!(viewer.cache.get(&survivor_a_id).unwrap().compiled.is_some());
        assert!(viewer.cache.get(&survivor_b_id).unwrap().compiled.is_some());

        // 记录编译后的表达式源码（用于验证缓存未被篡改）
        let survivor_a_source = viewer
            .cache
            .get(&survivor_a_id)
            .unwrap()
            .compiled
            .as_ref()
            .unwrap()
            .source()
            .to_string();
        let survivor_b_source = viewer
            .cache
            .get(&survivor_b_id)
            .unwrap()
            .compiled
            .as_ref()
            .unwrap()
            .source()
            .to_string();

        // 删除第一条曲线（索引 0）
        let deleted_id = viewer.curves[0].id;
        viewer.remove_curve_by_id(deleted_id);

        // 验证：剩余曲线的缓存数据未变
        let cache_a = viewer.cache.get(&survivor_a_id).unwrap();
        let cache_b = viewer.cache.get(&survivor_b_id).unwrap();

        assert!(
            cache_a.compiled.is_some(),
            "删除后 survivor_a 的编译结果不应丢失"
        );
        assert!(
            cache_b.compiled.is_some(),
            "删除后 survivor_b 的编译结果不应丢失"
        );
        assert_eq!(
            cache_a.compiled.as_ref().unwrap().source(),
            survivor_a_source,
            "survivor_a 的缓存表达式源码必须与删除前一致"
        );
        assert_eq!(
            cache_b.compiled.as_ref().unwrap().source(),
            survivor_b_source,
            "survivor_b 的缓存表达式源码必须与删除前一致"
        );

        // 验证：被删除曲线的缓存已被移除
        assert!(
            !viewer.cache.contains_key(&deleted_id),
            "被删除曲线的缓存条目必须移除"
        );
    }

    /// 删除中间曲线后，前后曲线的缓存数据均不变。
    #[test]
    fn test_delete_middle_preserves_neighbors() {
        let mut viewer = make_viewer();
        viewer.add_curve("sin(x)", palette_color(0));
        viewer.add_curve("cos(x)", palette_color(1));
        viewer.add_curve("x^2", palette_color(2));

        let first_id = viewer.curves[0].id;
        let middle_id = viewer.curves[1].id;
        let last_id = viewer.curves[2].id;

        // 记录删除前表达式
        let first_expr = viewer.curves[0].expression.clone();
        let last_expr = viewer.curves[2].expression.clone();

        // 删除中间曲线
        viewer.remove_curve_by_id(middle_id);

        // 验证前后曲线的表达式和缓存均未变
        assert_eq!(viewer.curves.len(), 2);
        assert_eq!(viewer.curves[0].id, first_id);
        assert_eq!(viewer.curves[0].expression, first_expr);
        assert_eq!(viewer.curves[1].id, last_id);
        assert_eq!(viewer.curves[1].expression, last_expr);

        // 缓存中仍能正确匹配
        assert!(viewer.cache.contains_key(&first_id));
        assert!(viewer.cache.contains_key(&last_id));
        assert!(!viewer.cache.contains_key(&middle_id));
    }

    /// 删除最后一条曲线后，前面曲线的缓存不变。
    #[test]
    fn test_delete_last_preserves_preceding() {
        let mut viewer = make_viewer();
        viewer.add_curve("sin(x)", palette_color(0));
        viewer.add_curve("cos(x)", palette_color(1));

        let first_id = viewer.curves[0].id;
        let last_id = viewer.curves[1].id;

        // 删除最后一条
        viewer.remove_curve_by_id(last_id);

        assert_eq!(viewer.curves.len(), 1);
        assert_eq!(viewer.curves[0].id, first_id);
        assert!(viewer.cache.contains_key(&first_id));
        assert!(!viewer.cache.contains_key(&last_id));
    }

    /// validate_cache 应清理所有残留的孤立缓存条目。
    #[test]
    fn test_validate_cache_removes_stale() {
        let mut viewer = make_viewer();
        viewer.add_curve("sin(x)", palette_color(0));
        viewer.add_curve("cos(x)", palette_color(1));

        // 手动注入一个孤立的缓存条目（模拟残留）
        let ghost_id = Uuid::new_v4();
        viewer.cache.insert(ghost_id, CurveCache::default());

        assert_eq!(viewer.cache.len(), 3);

        viewer.validate_cache();

        // 孤立条目应被移除
        assert_eq!(viewer.cache.len(), 2);
        assert!(!viewer.cache.contains_key(&ghost_id));
    }

    /// validate_cache 应为缺少缓存的曲线补建条目。
    #[test]
    fn test_validate_cache_adds_missing() {
        let mut viewer = make_viewer();
        viewer.add_curve("sin(x)", palette_color(0));
        viewer.add_curve("cos(x)", palette_color(1));

        // 手动移除一条缓存（模拟缓存丢失）
        let removed_id = viewer.curves[1].id;
        viewer.cache.remove(&removed_id);

        assert_eq!(viewer.cache.len(), 1);

        viewer.validate_cache();

        // 缺失的缓存应被补建
        assert_eq!(viewer.cache.len(), 2);
        assert!(viewer.cache.contains_key(&removed_id));
    }

    /// 【压力测试】多次增删后所有曲线位置和缓存均正确对应。
    #[test]
    fn test_multiple_add_delete_cycles() {
        let mut viewer = make_viewer();

        // 初始添加 5 条曲线
        let exprs = ["sin(x)", "cos(x)", "x^2", "x^3", "1/x"];
        for (i, expr) in exprs.iter().enumerate() {
            viewer.add_curve(expr, palette_color(i));
        }
        assert_eq!(viewer.curves.len(), 5);
        assert_eq!(viewer.cache.len(), 5);

        // 记录所有 ID → 表达式映射
        let id_expr_map: HashMap<Uuid, String> = viewer
            .curves
            .iter()
            .map(|c| (c.id, c.expression.clone()))
            .collect();

        // 删除第 1 条（索引 0）
        viewer.remove_curve_by_id(viewer.curves[0].id);
        assert_eq!(viewer.curves.len(), 4);

        // 再添加 2 条
        viewer.add_curve("sqrt(x)", palette_color(5));
        viewer.add_curve("log(x)", palette_color(6));
        assert_eq!(viewer.curves.len(), 6);

        // 删除中间某条
        let mid_id = viewer.curves[2].id;
        viewer.remove_curve_by_id(mid_id);
        assert_eq!(viewer.curves.len(), 5);

        // 删除最后一条
        let last_id = viewer.curves[4].id;
        viewer.remove_curve_by_id(last_id);
        assert_eq!(viewer.curves.len(), 4);

        // 验证：所有存活曲线的 UUID → 表达式映射与原始记录一致
        for curve in &viewer.curves {
            if let Some(original_expr) = id_expr_map.get(&curve.id) {
                assert_eq!(
                    &curve.expression, original_expr,
                    "存活曲线的表达式必须与原始记录一致"
                );
            }
            // 每条存活曲线都应有缓存条目
            assert!(
                viewer.cache.contains_key(&curve.id),
                "每条存活曲线都应有缓存条目"
            );
        }

        // 验证：缓存条目数 == 曲线数
        assert_eq!(
            viewer.cache.len(),
            viewer.curves.len(),
            "缓存条目数必须等于曲线数"
        );

        // 验证：没有孤立的缓存条目
        for cache_id in viewer.cache.keys() {
            assert!(
                viewer.curves.iter().any(|c| c.id == *cache_id),
                "缓存中不应存在已被删除曲线的孤立条目"
            );
        }
    }

    /// 删除所有曲线后缓存应为空。
    #[test]
    fn test_delete_all_clears_cache() {
        let mut viewer = make_viewer();
        viewer.add_curve("sin(x)", palette_color(0));
        viewer.add_curve("cos(x)", palette_color(1));

        let ids: Vec<Uuid> = viewer.curves.iter().map(|c| c.id).collect();
        for id in ids {
            viewer.remove_curve_by_id(id);
        }

        assert!(viewer.curves.is_empty());
        assert!(viewer.cache.is_empty());
    }

    /// 删除后重新添加相同表达式，新曲线应获得新的 UUID 和独立缓存。
    #[test]
    fn test_readd_gets_new_uuid_and_cache() {
        let mut viewer = make_viewer();
        viewer.add_curve("sin(x)", palette_color(0));
        let original_id = viewer.curves[0].id;

        viewer.remove_curve_by_id(original_id);
        viewer.add_curve("sin(x)", palette_color(0));

        let new_id = viewer.curves[0].id;
        assert_ne!(new_id, original_id, "重新添加的曲线必须获得新的 UUID");
        assert!(viewer.cache.contains_key(&new_id));
        assert!(!viewer.cache.contains_key(&original_id));
    }

    /// 编辑曲线后缓存应更新为新的编译结果。
    #[test]
    fn test_edit_updates_cache() {
        let mut viewer = make_viewer();
        viewer.add_curve("sin(x)", palette_color(0));
        let curve_id = viewer.curves[0].id;

        let original_source = viewer
            .cache
            .get(&curve_id)
            .unwrap()
            .compiled
            .as_ref()
            .unwrap()
            .source()
            .to_string();
        assert_eq!(original_source, "sin(x)");

        // 修改表达式
        viewer.curves[0].expression = "cos(x)".to_string();
        viewer.recompile_by_id(curve_id);

        let new_source = viewer
            .cache
            .get(&curve_id)
            .unwrap()
            .compiled
            .as_ref()
            .unwrap()
            .source()
            .to_string();
        assert_eq!(new_source, "cos(x)");
    }

    /// 参数变更后曲线缓存应标记为 dirty。
    #[test]
    fn test_param_change_marks_dirty() {
        let mut viewer = make_viewer();
        viewer.add_curve("a * sin(x)", palette_color(0));
        let curve_id = viewer.curves[0].id;

        // 初始缓存不 dirty
        let entry = viewer.cache.get(&curve_id).unwrap();
        assert!(!entry.dirty, "新编译的缓存初始不应为 dirty");

        // 模拟参数变更
        if let Some(entry) = viewer.cache.get_mut(&curve_id) {
            entry.dirty = true;
        }

        let entry = viewer.cache.get(&curve_id).unwrap();
        assert!(entry.dirty, "参数变更后缓存应标记为 dirty");
    }
}
