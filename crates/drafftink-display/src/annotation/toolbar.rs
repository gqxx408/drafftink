use std::collections::HashMap;

use egui::{Align, Align2, Area, Color32, Context, Id, Layout, Order, Slider, Stroke};

use super::stroke::ToolType;

const PEN_COLORS: [([u8; 3], &str); 6] = [
    ([0, 0, 0], "黑"),
    ([220, 50, 50], "红"),
    ([50, 150, 50], "绿"),
    ([50, 100, 220], "蓝"),
    ([255, 140, 0], "橙"),
    ([128, 0, 128], "紫"),
];

const HIGHLIGHTER_COLORS: [([u8; 3], &str); 5] = [
    ([255, 230, 0], "黄"),
    ([50, 200, 80], "绿"),
    ([255, 80, 160], "粉"),
    ([30, 120, 255], "蓝"),
    ([255, 140, 0], "橙"),
];

const PEN_WIDTHS: [(f32, &str); 4] = [(1.5, "细"), (2.5, "中"), (4.0, "粗"), (6.0, "特粗")];

const HIGHLIGHTER_WIDTHS: [(f32, &str); 4] =
    [(8.0, "细"), (12.0, "中"), (18.0, "粗"), (25.0, "特粗")];

const ACTIVE_BG: Color32 = Color32::from_rgb(60, 120, 220);
const TOOLBAR_BG: Color32 = Color32::from_rgba_premultiplied(245, 245, 245, 255);
const TOOLBAR_TEXT: Color32 = Color32::from_rgb(30, 30, 30);
const TOOLBAR_BORDER: Color32 = Color32::from_rgb(180, 180, 180);
const TOOLBAR_INACTIVE_BG: Color32 = Color32::from_rgb(220, 220, 220);

#[derive(Debug, PartialEq)]
pub enum ToolbarAction {
    None,
    NewPage,
    PrevPage,
    NextPage,
    Exit,
    ToggleMore,
}

pub struct AnnotationToolbar {
    pub visible: bool,
    auto_hide_seconds: f64,
    last_interaction: f64,
    pub more_menu_open: bool,
    highlighter_alphas: HashMap<[u8; 3], u8>,
    smart_alpha_enabled: bool,
}

impl Default for AnnotationToolbar {
    fn default() -> Self {
        let mut alphas = HashMap::new();
        for (c, _) in HIGHLIGHTER_COLORS {
            alphas.insert(c, 140);
        }
        Self {
            visible: true,
            auto_hide_seconds: 3.0,
            last_interaction: 0.0,
            more_menu_open: false,
            highlighter_alphas: alphas,
            smart_alpha_enabled: false,
        }
    }
}

impl AnnotationToolbar {
    pub fn update(
        &mut self,
        ctx: &Context,
        tool: &mut ToolType,
        color: &mut [u8; 4],
        thickness: &mut f32,
        page_current: usize,
        page_total: usize,
        toolbar_changed: &mut bool,
        smart_alpha: &super::smart_alpha::SmartAlpha,
    ) -> ToolbarAction {
        let mut action = ToolbarAction::None;
        *toolbar_changed = false;

        let input = ctx.input(|i| i.clone());
        let now = input.time;
        if input.pointer.delta().length() > 1.0 || input.pointer.any_pressed() {
            self.last_interaction = now;
            self.visible = true;
        }
        if now - self.last_interaction > self.auto_hide_seconds {
            self.visible = false;
        }

        // Keyboard: arrow up/down → adjust highlighter alpha
        if matches!(tool, ToolType::Highlighter) {
            let rgb: [u8; 3] = [color[0], color[1], color[2]];
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                let a = ((color[3] as i32 + 5).min(255)) as u8;
                self.highlighter_alphas.insert(rgb, a);
                color[3] = a;
                *toolbar_changed = true;
                self.last_interaction = now;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                let a = ((color[3] as i32 - 5).max(0)) as u8;
                self.highlighter_alphas.insert(rgb, a);
                color[3] = a;
                *toolbar_changed = true;
                self.last_interaction = now;
            }
        }

        // Shortcuts
        if ctx.input(|i| i.key_pressed(egui::Key::P)) {
            *tool = ToolType::Pen;
            *color = [0, 0, 0, 255];
            *thickness = 2.5;
            *toolbar_changed = true;
            self.last_interaction = now;
            self.visible = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::H)) {
            *tool = ToolType::Highlighter;
            let hl: [u8; 3] = [255, 230, 0];
            let a = *self.highlighter_alphas.get(&hl).unwrap_or(&140);
            *color = [hl[0], hl[1], hl[2], a];
            *thickness = 12.0;
            *toolbar_changed = true;
            self.last_interaction = now;
            self.visible = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::E)) {
            *tool = ToolType::Eraser;
            *thickness = 12.0;
            *toolbar_changed = true;
            self.last_interaction = now;
            self.visible = true;
        }

        if self.visible {
            self.render(
                ctx,
                tool,
                color,
                thickness,
                page_current,
                page_total,
                toolbar_changed,
                &mut action,
                smart_alpha,
            );
        }

        action
    }

    #[allow(clippy::too_many_arguments)]
    fn render(
        &mut self,
        ctx: &Context,
        tool: &mut ToolType,
        color: &mut [u8; 4],
        thickness: &mut f32,
        page_current: usize,
        page_total: usize,
        changed: &mut bool,
        action: &mut ToolbarAction,
        smart_alpha: &super::smart_alpha::SmartAlpha,
    ) {
        // 底边栏的尺寸必须由**屏幕**推导，绝不能由 `ui.available_width()` 反推。
        //
        // `Area` 交给内容的 `max_rect` 取自它 *上一帧* 的尺寸（`AreaState::rect()`），
        // 所以「读可用宽度 → 撑一个 spacer → 决定本帧宽度」会形成正反馈闭环：
        // 本帧算出的宽度成为下一帧的输入，每帧多撑十几像素，底边栏持续横向拉伸，
        // 最终把右侧的「退出 / 更多」顶出屏幕之外，再也点不到。
        //
        // 钉死宽高后，布局在帧与帧之间是幂等的，闭环被切断。
        const BAR_MARGIN: f32 = 6.0;
        // 行内最高的控件是 32×32 的工具按钮。
        const ROW_HEIGHT: f32 = 32.0;
        let inner_width = (ctx.screen_rect().width() - 2.0 * BAR_MARGIN).max(0.0);

        Area::new(Id::new("annotation_toolbar"))
            .anchor(Align2::CENTER_BOTTOM, [0.0, 0.0])
            .order(Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(TOOLBAR_BG)
                    .inner_margin(egui::Margin::same(BAR_MARGIN))
                    .show(ui, |ui| {
                        // 覆盖默认深色主题：白底工具栏需要黑字
                        ui.visuals_mut().widgets.noninteractive.fg_stroke.color = TOOLBAR_TEXT;
                        ui.visuals_mut().widgets.noninteractive.bg_stroke =
                            Stroke::new(1.0_f32, TOOLBAR_BORDER);
                        ui.visuals_mut().widgets.inactive.fg_stroke.color = TOOLBAR_TEXT;
                        ui.visuals_mut().widgets.hovered.fg_stroke.color = TOOLBAR_TEXT;
                        ui.visuals_mut().widgets.active.fg_stroke.color = TOOLBAR_TEXT;
                        ui.visuals_mut().widgets.inactive.bg_fill = TOOLBAR_INACTIVE_BG;
                        ui.visuals_mut().widgets.inactive.bg_stroke =
                            Stroke::new(1.0_f32, TOOLBAR_BORDER);
                        ui.visuals_mut().widgets.hovered.bg_fill = TOOLBAR_INACTIVE_BG;
                        ui.visuals_mut().widgets.active.bg_fill = ACTIVE_BG;
                        ui.visuals_mut().widgets.active.fg_stroke.color = Color32::WHITE;

                        // 横向：钉死为屏幕宽度，切断上面说明的自反馈闭环。
                        ui.set_width(inner_width);
                        // 纵向：水平布局里的 `ui.separator()` 会吃掉**全部可用高度**，
                        // 而这个可用高度同样来自上一帧，是同一类隐患——一旦某帧被撑高
                        // 就再也回不去。显式钉死行高即可根除。
                        ui.set_height(ROW_HEIGHT);

                        ui.horizontal(|ui| {
                            // ── Page nav ──
                            if ui
                                .add(
                                    egui::Button::new("新建画布")
                                        .fill(Color32::TRANSPARENT)
                                        .stroke(Stroke::NONE)
                                        .min_size(egui::vec2(60.0, 28.0)),
                                )
                                .clicked()
                            {
                                *action = ToolbarAction::NewPage;
                            }
                            if ui
                                .add(
                                    egui::Button::new("◀")
                                        .fill(Color32::TRANSPARENT)
                                        .stroke(Stroke::NONE)
                                        .min_size(egui::vec2(22.0, 28.0)),
                                )
                                .clicked()
                                && page_current > 0 {
                                    *action = ToolbarAction::PrevPage;
                                }
                            ui.label(
                                egui::RichText::new(format!(
                                    "{}/{}",
                                    page_current + 1,
                                    page_total.max(1)
                                ))
                                .size(13.0)
                                .color(TOOLBAR_TEXT),
                            );
                            if ui
                                .add(
                                    egui::Button::new("▶")
                                        .fill(Color32::TRANSPARENT)
                                        .stroke(Stroke::NONE)
                                        .min_size(egui::vec2(22.0, 28.0)),
                                )
                                .clicked()
                                && page_current + 1 < page_total {
                                    *action = ToolbarAction::NextPage;
                                }

                            ui.separator();

                            // ── Tool buttons ──
                            let pen_active = matches!(tool, ToolType::Pen);
                            if ui
                                .add(
                                    egui::Button::new("✏")
                                        .fill(if pen_active {
                                            ACTIVE_BG
                                        } else {
                                            Color32::TRANSPARENT
                                        })
                                        .stroke(Stroke::new(1.0_f32, TOOLBAR_BORDER))
                                        .min_size(egui::vec2(32.0, 32.0)),
                                )
                                .on_hover_text("笔 (P)")
                                .clicked()
                            {
                                *tool = ToolType::Pen;
                                *color = [0, 0, 0, 255];
                                *thickness = 2.5;
                                *changed = true;
                            }

                            let hl_active = matches!(tool, ToolType::Highlighter);
                            if ui
                                .add(
                                    egui::Button::new("🖍")
                                        .fill(if hl_active {
                                            ACTIVE_BG
                                        } else {
                                            Color32::TRANSPARENT
                                        })
                                        .stroke(Stroke::new(1.0_f32, TOOLBAR_BORDER))
                                        .min_size(egui::vec2(32.0, 32.0)),
                                )
                                .on_hover_text("荧光笔 (H)")
                                .clicked()
                            {
                                *tool = ToolType::Highlighter;
                                let hl: [u8; 3] = [255, 230, 0];
                                let a = *self.highlighter_alphas.get(&hl).unwrap_or(&140);
                                *color = [hl[0], hl[1], hl[2], a];
                                *thickness = 12.0;
                                *changed = true;
                            }

                            let er_active = matches!(tool, ToolType::Eraser);
                            if ui
                                .add(
                                    egui::Button::new("✕")
                                        .fill(if er_active {
                                            ACTIVE_BG
                                        } else {
                                            Color32::TRANSPARENT
                                        })
                                        .stroke(Stroke::new(1.0_f32, TOOLBAR_BORDER))
                                        .min_size(egui::vec2(32.0, 32.0)),
                                )
                                .on_hover_text("橡皮擦 (E)")
                                .clicked()
                            {
                                *tool = ToolType::Eraser;
                                *thickness = 12.0;
                                *changed = true;
                            }

                            ui.separator();

                            // ── Color swatches ──
                            if matches!(tool, ToolType::Highlighter) {
                                for &(c, _) in HIGHLIGHTER_COLORS.iter() {
                                    let is_current = color[0..3] == c;
                                    let fill =
                                        Color32::from_rgba_premultiplied(c[0], c[1], c[2], 255);
                                    if ui
                                        .add(
                                            egui::Button::new("")
                                                .fill(fill)
                                                .stroke(Stroke::new(
                                                    if is_current { 3.0_f32 } else { 1.0_f32 },
                                                    if is_current {
                                                        Color32::WHITE
                                                    } else {
                                                        TOOLBAR_BORDER
                                                    },
                                                ))
                                                .min_size(egui::vec2(22.0, 22.0)),
                                        )
                                        .clicked()
                                    {
                                        let a = *self.highlighter_alphas.get(&c).unwrap_or(&140);
                                        color[0] = c[0];
                                        color[1] = c[1];
                                        color[2] = c[2];
                                        color[3] = a;
                                        *changed = true;
                                    }
                                    ui.add_space(2.0);
                                }
                            } else {
                                for &(c, _) in PEN_COLORS.iter() {
                                    let expected: [u8; 4] = [c[0], c[1], c[2], 255];
                                    let is_current = *color == expected;
                                    let fill =
                                        Color32::from_rgba_premultiplied(c[0], c[1], c[2], 255);
                                    if ui
                                        .add(
                                            egui::Button::new("")
                                                .fill(fill)
                                                .stroke(Stroke::new(
                                                    if is_current { 3.0_f32 } else { 1.0_f32 },
                                                    if is_current {
                                                        Color32::WHITE
                                                    } else {
                                                        TOOLBAR_BORDER
                                                    },
                                                ))
                                                .min_size(egui::vec2(22.0, 22.0)),
                                        )
                                        .clicked()
                                    {
                                        *color = expected;
                                        *changed = true;
                                    }
                                    ui.add_space(2.0);
                                }
                            }

                            ui.separator();

                            // ── Thickness ──
                            let widths: &[(f32, &str)] = match tool {
                                ToolType::Highlighter => &HIGHLIGHTER_WIDTHS,
                                _ => &PEN_WIDTHS,
                            };
                            for (w, label) in widths {
                                let is_current = (*thickness - w).abs() < 0.1;
                                if ui
                                    .add(
                                        egui::Button::new(*label)
                                            .fill(if is_current {
                                                ACTIVE_BG
                                            } else {
                                                TOOLBAR_INACTIVE_BG
                                            })
                                            .min_size(egui::vec2(36.0, 24.0)),
                                    )
                                    .clicked()
                                {
                                    *thickness = *w;
                                    *changed = true;
                                }
                            }

                            // ── Alpha slider (highlighter only) ──
                            if matches!(tool, ToolType::Highlighter) {
                                ui.separator();
                                ui.label(
                                    egui::RichText::new(format!("透明:{}", color[3]))
                                        .size(11.0)
                                        .color(TOOLBAR_TEXT),
                                );
                                let mut a = color[3] as f64;
                                if ui
                                    .add(
                                        Slider::new(&mut a, 0.0..=255.0)
                                            .step_by(5.0)
                                            .show_value(false)
                                            .fixed_decimals(0),
                                    )
                                    .changed()
                                {
                                    let rgb: [u8; 3] = [color[0], color[1], color[2]];
                                    let na = a as u8;
                                    self.highlighter_alphas.insert(rgb, na);
                                    color[3] = na;
                                    *changed = true;
                                }
                                ui.label("↑↓").on_hover_text("键盘上下键微调透明度");

                                // Smart alpha button
                                ui.add_space(4.0);
                                let sa_enabled = self.smart_alpha_enabled;
                                if ui
                                    .add(
                                        egui::Button::new(if sa_enabled {
                                            "🎯✓"
                                        } else {
                                            "🎯智能"
                                        })
                                        .fill(if sa_enabled {
                                            Color32::from_rgb(60, 180, 100)
                                        } else {
                                            TOOLBAR_INACTIVE_BG
                                        })
                                        .min_size(egui::vec2(48.0, 22.0)),
                                    )
                                    .clicked()
                                {
                                    self.smart_alpha_enabled = !sa_enabled;
                                    if !sa_enabled {
                                        let rgb: [u8; 3] = [color[0], color[1], color[2]];
                                        if let Some(rec) = smart_alpha.recommendation_for(&rgb) {
                                            self.highlighter_alphas.insert(rgb, rec);
                                            color[3] = rec;
                                            *changed = true;
                                        }
                                    }
                                }

                                // Recommendation hint
                                if self.smart_alpha_enabled {
                                    let rgb: [u8; 3] = [color[0], color[1], color[2]];
                                    if let Some(rec) = smart_alpha.recommendation_for(&rgb) {
                                        ui.label(
                                            egui::RichText::new(format!("→{}", rec))
                                                .size(11.0)
                                                .color(Color32::from_rgb(100, 200, 100)),
                                        );
                                    }
                                }
                            }

                            ui.separator();

                            // ── Right: more + exit ──
                            //
                            // 用反向布局贴住右端，而不是先量 `available_width()` 再撑一个
                            // spacer。反向布局占用的是**本行剩余空间**，不会把本帧的测量
                            // 结果写回下一帧，因此宽度不会逐帧累积。
                            //
                            // 反向布局中先添加的控件位于最右，故顺序与视觉顺序相反：
                            // 先「更多」（最右），后「退出」。
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui
                                    .add(
                                        egui::Button::new(if self.more_menu_open {
                                            "更多 ▴"
                                        } else {
                                            "更多 ▾"
                                        })
                                        .fill(Color32::TRANSPARENT)
                                        .stroke(Stroke::NONE)
                                        .min_size(egui::vec2(52.0, 28.0)),
                                    )
                                    .clicked()
                                {
                                    *action = ToolbarAction::ToggleMore;
                                }
                                if ui
                                    .add(
                                        egui::Button::new("退出")
                                            .fill(Color32::TRANSPARENT)
                                            .stroke(Stroke::NONE)
                                            .min_size(egui::vec2(36.0, 28.0)),
                                    )
                                    .clicked()
                                {
                                    *action = ToolbarAction::Exit;
                                }
                            });
                        });
                    });
            });
    }
}
