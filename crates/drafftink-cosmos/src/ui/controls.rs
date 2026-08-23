//! 控制面板 UI：时间控制、视角切换、行星选择等

use egui::{Color32, RichText, Stroke};

use crate::scene::SolarSystemScene;

/// 视图模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    /// 3D 立体视图
    Mode3D,
    /// 2D 地图视图
    Mode2D,
}

impl Default for ViewMode {
    fn default() -> Self {
        ViewMode::Mode3D
    }
}

/// 控制栏状态
///
/// 管理所有 UI 交互状态，包括视图模式、标签显示、轨道显示、
/// 模拟速度、暂停状态和选中的行星。
#[derive(Debug, Clone)]
pub struct ControlPanel {
    /// 当前视图模式（3D / 2D）
    pub view_mode: ViewMode,
    /// 是否显示标签
    pub show_labels: bool,
    /// 是否显示轨道线
    pub show_orbits: bool,
    /// 模拟速度倍率（0.1 ~ 10.0）
    pub simulation_speed: f32,
    /// 是否暂停模拟
    pub paused: bool,
    /// 当前选中的行星索引（None 表示未选中）
    pub selected_planet: Option<usize>,
    /// 是否请求重置视角
    pub reset_view_requested: bool,
    /// 轨道显示状态变化标记（用于缓存失效）
    show_orbits_changed: bool,
    /// 缓存的轨道显示状态（用于检测变化）
    last_show_orbits: bool,
    // ── 性能优化：行星标签缓存 ──
    /// 缓存的行星列表标签对：(选中标签, 未选中标签)
    /// 避免每帧 `format!()` 分配
    cached_planet_labels: Vec<(String, String)>,
    /// 缓存对应的场景实体数量（用于检测场景变化）
    cached_entity_count: usize,
}

impl ControlPanel {
    /// 创建默认控制面板
    ///
    /// 默认值：3D 模式，显示标签，显示轨道，速度 1.0，不暂停，未选中行星。
    pub fn new() -> Self {
        Self {
            view_mode: ViewMode::Mode3D,
            show_labels: true,
            show_orbits: true,
            simulation_speed: 1.0,
            paused: false,
            selected_planet: None,
            reset_view_requested: false,
            show_orbits_changed: false,
            last_show_orbits: true,
            cached_planet_labels: Vec::new(),
            cached_entity_count: 0,
        }
    }

    /// 绘制左上角浮动控制面板
    ///
    /// 使用 egui::Area 绘制可拖动的浮动面板，包含：
    /// - 视图模式切换（3D / 2D）
    /// - 显示选项（标签、轨道、暂停）
    /// - 模拟速度滑块
    /// - 行星列表
    /// - 重置视角按钮
    pub fn ui(&mut self, ctx: &egui::Context, scene: &SolarSystemScene) {
        let panel_frame = egui::Frame::window(&ctx.style())
            .inner_margin(egui::Margin::symmetric(12.0, 10.0))
            .fill(Color32::from_black_alpha(200))
            .stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(100, 150, 200, 180)));

        egui::Area::new(egui::Id::new("cosmos_controls"))
            .movable(true)
            .anchor(egui::Align2::LEFT_TOP, egui::vec2(12.0, 12.0))
            .show(ctx, |ui| {
                panel_frame.show(ui, |ui| {
                    ui.set_max_width(240.0);

                    // 标题
                    ui.heading(
                        RichText::new("\u{1F30C} 太阳系视图")
                            .color(Color32::from_rgb(150, 200, 255))
                            .size(16.0),
                    );
                    ui.add_space(6.0);

                    // ---- 视图模式切换 ----
                    ui.label(RichText::new("视图模式").color(Color32::from_rgb(200, 200, 200)));
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.view_mode, ViewMode::Mode3D, "3D 视图");
                        ui.selectable_value(&mut self.view_mode, ViewMode::Mode2D, "2D 地图");
                    });
                    ui.add_space(6.0);

                    // ---- 显示选项 ----
                    ui.label(RichText::new("显示选项").color(Color32::from_rgb(200, 200, 200)));
                    ui.checkbox(&mut self.show_labels, "显示标签");
                    ui.checkbox(&mut self.show_orbits, "显示轨道");
                    ui.checkbox(&mut self.paused, "暂停模拟");
                    ui.add_space(6.0);

                    // ---- 模拟速度滑块 ----
                    ui.label(RichText::new("模拟速度").color(Color32::from_rgb(200, 200, 200)));
                    ui.add(
                        egui::Slider::new(&mut self.simulation_speed, 0.1..=10.0)
                            .logarithmic(true)
                            .fixed_decimals(1)
                            .suffix("x"),
                    );
                    ui.add_space(6.0);

                    // ---- 行星列表 ----
                    ui.label(RichText::new("行星列表").color(Color32::from_rgb(200, 200, 200)));

                    // ── 性能优化：缓存标签，仅场景变化时重建 ──
                    let entity_count = scene.names.len();
                    if self.cached_planet_labels.len() != entity_count {
                        self.cached_planet_labels = scene.names
                            .iter()
                            .enumerate()
                            .map(|(idx, _name)| {
                                let name = scene.planet_infos[idx]
                                    .as_ref()
                                    .map(|info| info.name.as_str())
                                    .unwrap_or("?");
                                (
                                    format!("\u{25CF} {}", name),
                                    format!("\u{25CB} {}", name),
                                )
                            })
                            .collect();
                        self.cached_entity_count = entity_count;
                    }

                    egui::ScrollArea::vertical()
                        .max_height(180.0)
                        .show(ui, |ui| {
                            for (idx, _name) in scene.names.iter().enumerate() {
                                // 跳过没有行星信息的实体（如土星环）
                                if scene.planet_infos[idx].is_none() {
                                    continue;
                                }

                                let is_selected = self.selected_planet == Some(idx);

                                // 从缓存取标签，避免每帧 format!()
                                let label_text = if idx < self.cached_planet_labels.len() {
                                    let (ref selected, ref unselected) = self.cached_planet_labels[idx];
                                    if is_selected { selected.as_str() } else { unselected.as_str() }
                                } else {
                                    // 防御性回退
                                    ""
                                };

                                let response = ui.selectable_label(is_selected, label_text);

                                if response.clicked() {
                                    if is_selected {
                                        self.selected_planet = None;
                                    } else {
                                        self.selected_planet = Some(idx);
                                    }
                                }
                            }
                        });
                    ui.add_space(6.0);

                    // ---- 选中行星详情 ----
                    if let Some(idx) = self.selected_planet {
                        if let Some(info) = &scene.planet_infos[idx] {
                            ui.separator();
                            ui.label(
                                RichText::new(&info.name)
                                    .color(Color32::from_rgb(255, 220, 150))
                                    .size(14.0)
                                    .strong(),
                            );
                            ui.label(
                                RichText::new(&info.description)
                                    .color(Color32::from_rgb(180, 180, 180))
                                    .size(11.0),
                            );
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(format!(
                                    "直径: {:.0} km",
                                    info.diameter_km
                                ))
                                .color(Color32::from_rgb(160, 180, 200))
                                .size(10.0),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "质量: {:.2e} kg",
                                    info.mass_kg
                                ))
                                .color(Color32::from_rgb(160, 180, 200))
                                .size(10.0),
                            );
                        }
                    }

                    ui.add_space(6.0);
                    ui.separator();

                    // ---- 重置视角按钮 ----
                    if ui.button("\u{1F504} 重置视角").clicked() {
                        self.reset_view_requested = true;
                    }
                });
            });
    }

    /// 检查并消费重置视角请求
    ///
    /// 返回 true 表示需要重置视角，并清除请求标志。
    pub fn take_reset_view_request(&mut self) -> bool {
        let requested = self.reset_view_requested;
        self.reset_view_requested = false;
        requested
    }

    /// 检查轨道显示状态是否发生变化，并消费变化标记。
    ///
    /// 用于触发轨道线缓存失效。
    pub fn show_orbits_changed(&mut self) -> bool {
        let changed = self.show_orbits != self.last_show_orbits;
        self.last_show_orbits = self.show_orbits;
        self.show_orbits_changed = false;
        changed
    }
}

impl Default for ControlPanel {
    fn default() -> Self {
        Self::new()
    }
}
