//! SeewoClass MVP — main application and UI layout.
#![allow(dead_code)]
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use drafftink_core::history::History;
use drafftink_core::model::{CoursewareDoc, ShapeType};
use drafftink_core::Camera;
use egui::{Color32, Key, KeyboardShortcut, Modifiers, Sense, Ui};
use raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, WindowHandle};

use drafftink_quiz::actors::ui::UiState;
use drafftink_quiz::messages::SessionCommand;
use drafftink_quiz::ui::QuizPanel;
use drafftink_quiz::QuizConfig;

use crate::animation_player::AnimationPlayer;
use crate::annotation::{AnnotationState, AnnotationTool, ERASER_MAX_SIZE, ERASER_MIN_SIZE};
use crate::interaction::{InteractionState, ToolMode};
use crate::multi_page::MultiPageState;
use crate::{io, render};
use drafftink_core::board::{ActiveBoard, DisplayBoard, EditBoard, Snapshot, StandbySnapshot};

// ── Colour constants ───────────────────────────────────────────────────────
const TOOLBAR_BG: Color32 = Color32::from_rgb(0x3C, 0x3C, 0x3C);
const SIDEBAR_BG: Color32 = Color32::from_rgb(0xF5, 0xF5, 0xF5);
const CANVAS_BG: Color32 = Color32::from_rgb(0xE0, 0xE0, 0xE0);
const PAGE_ACTIVE: Color32 = Color32::from_rgb(0x00, 0xC8, 0x00);
const PAGE_INACTIVE: Color32 = Color32::from_rgb(0xD0, 0xD0, 0xD0);

// ---------------------------------------------------------------------------
// ParentWindow — 包装 RawWindowHandle 用于 rfd::FileDialog::set_parent()
// ---------------------------------------------------------------------------

/// 包装 RawWindowHandle，使文件对话框始终绑定到主窗口，
/// 防止对话框被主窗口遮挡。
#[derive(Clone, Copy)]
struct ParentWindow(RawWindowHandle);

// SAFETY: 句柄来自存活的 eframe 窗口，在应用生命周期内始终有效。
impl HasWindowHandle for ParentWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        // SAFETY: 句柄在应用生命周期内始终有效。
        Ok(unsafe { WindowHandle::borrow_raw(self.0) })
    }
}

// ==========================================================================
// App
// ==========================================================================

pub struct SeewoClassApp {
    pub doc: CoursewareDoc,
    pub camera: Camera,
    pub interaction: InteractionState,
    pub history: History,
    pub annotation: AnnotationState,
    pub multi_page: MultiPageState,
    pub animation_player: AnimationPlayer,
    pub current: ActiveBoard,
    pub standby: StandbySnapshot,

    pub current_path: Option<PathBuf>,
    pub show_export_dialog: bool,
    pub export_width: u32,
    pub export_height: u32,
    pub show_color_panel: bool,
    pub show_more_panel: bool,
    pub show_quiz_panel: bool,
    pub toast_message: String,
    pub toast_timer: f32,
    pub status_message: String,

    // Plugin
    pub enbx_plugin: Option<()>,

    // Quiz system
    pub quiz_ui_state: Option<Arc<Mutex<UiState>>>,
    pub quiz_session_tx: Option<tokio::sync::mpsc::Sender<SessionCommand>>,
    pub quiz_panel: QuizPanel,
    quiz_runtime: Option<tokio::runtime::Runtime>,
    /// 父窗口句柄（用于文件对话框 set_parent，防止遮挡）
    parent_window: Option<ParentWindow>,
}

impl Default for SeewoClassApp {
    fn default() -> Self {
        let (_doc_width, _doc_height) = (1920.0, 1080.0);
        Self {
            doc: CoursewareDoc::default(),
            camera: Camera::default(),
            interaction: InteractionState::new(),
            history: History::new(),
            annotation: AnnotationState::new(),
            multi_page: MultiPageState::new(),
            animation_player: AnimationPlayer::new(),
            current: ActiveBoard::default(),
            standby: StandbySnapshot::default(),
            current_path: None,
            show_export_dialog: false,
            export_width: 1920,
            export_height: 1080,
            show_color_panel: false,
            show_more_panel: false,
            show_quiz_panel: false,
            toast_message: String::new(),
            toast_timer: 0.0,
            status_message: String::new(),
            enbx_plugin: None,
            quiz_ui_state: None,
            quiz_session_tx: None,
            quiz_panel: QuizPanel::default(),
            quiz_runtime: None,
            parent_window: None,
        }
    }
}

impl SeewoClassApp {
    // ================================================================
    // Style
    // ================================================================

    fn apply_style(&self, ctx: &egui::Context) {
        // ── 强制深色主题：覆盖 Windows 系统主题偏好，防止浅色模式下文字不可见 ──
        ctx.set_theme(egui::ThemePreference::Dark);

        let is_edit = matches!(&self.current, ActiveBoard::Edit(_));
        let mut s = (*ctx.style()).clone();
        // 显式设置深色 Visuals 作为备选方案，确保 ThemePreference 生效
        s.visuals = egui::Visuals::dark();
        s.visuals.window_rounding = egui::Rounding::same(4.0);
        if is_edit {
            s.visuals.window_fill = CANVAS_BG;
        }
        ctx.set_style(s);
    }

    // ================================================================
    // Shortcuts
    // ================================================================

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        ctx.input_mut(|i| {
            if i.consume_shortcut(&KeyboardShortcut::new(Modifiers::CTRL, Key::Z)) {
                self.undo();
            }
            if i.consume_shortcut(&KeyboardShortcut::new(
                Modifiers::CTRL.plus(Modifiers::SHIFT),
                Key::Z,
            )) || i.consume_shortcut(&KeyboardShortcut::new(Modifiers::CTRL, Key::Y))
            {
                self.redo();
            }
            if i.consume_shortcut(&KeyboardShortcut::new(Modifiers::CTRL, Key::S)) {
                self.save_action();
            }
            if i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace) {
                self.delete_selected();
            }
            if i.consume_shortcut(&KeyboardShortcut::new(Modifiers::CTRL, Key::A)) {
                self.interaction.selected_ids = self.doc.elements.iter().map(|e| e.id()).collect();
            }
            if i.key_pressed(Key::Escape) {
                self.interaction.clear_selection();
                self.interaction.editing_text_id = None;
            }
        });
    }

    fn handle_drop(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in &dropped {
            if let Some(p) = &file.path {
                self.import_enbx_path(p);
                break;
            }
        }
    }

    fn handle_eraser_scroll(&mut self, ctx: &egui::Context) {
        if !matches!(self.annotation.current_tool, AnnotationTool::Eraser) {
            return;
        }
        let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let new = self.annotation.eraser_size + scroll * 0.5;
            self.annotation.eraser_size = new.clamp(ERASER_MIN_SIZE, ERASER_MAX_SIZE);
        }
    }

    fn toggle_mode(&mut self) {
        match &self.current {
            ActiveBoard::Display(_) => self.enter_edit_mode(),
            ActiveBoard::Edit(_) => self.enter_display_mode(),
        }
    }

    fn enter_edit_mode(&mut self) {
        log::info!("Switching to Edit mode");
        self.animation_player.shutdown();
        let idx = self.multi_page.current_page;
        let disp_elements = match &self.current {
            ActiveBoard::Display(d) => d.elements.clone(),
            _ => return,
        };
        if let Some(page) = self.doc.pages.get_mut(idx) {
            page.elements = disp_elements.clone();
        }
        self.standby = StandbySnapshot::Display(disp_elements);
        let snap = Snapshot::from_doc(&self.doc, idx);
        let mut edit = EditBoard::default();
        edit.load_snapshot(&snap);
        self.current = ActiveBoard::Edit(edit);
    }

    fn enter_display_mode(&mut self) {
        log::info!("Switching to Display mode");
        let idx = self.multi_page.current_page;
        let edit_elements = match &self.current {
            ActiveBoard::Edit(e) => e.elements.clone(),
            _ => return,
        };
        if let Some(page) = self.doc.pages.get_mut(idx) {
            page.elements = edit_elements.clone();
        }
        self.standby = StandbySnapshot::Edit(edit_elements);
        let snap = Snapshot::from_doc(&self.doc, idx);
        let mut display = DisplayBoard::default();
        display.load_snapshot(&snap);
        let size = self.doc.page_size;
        if let Some(page) = self.doc.pages.get_mut(idx) {
            if let Some(ref seq) = page.animation_sequence {
                self.animation_player.init_page(
                    seq.clone(),
                    page.animations.clone(),
                    size,
                    &mut page.elements,
                );
            }
        }
        self.current = ActiveBoard::Display(display);
    }

    fn init_page_animations(&mut self) {
        self.animation_player.shutdown();
        let idx = self.multi_page.current_page;
        let size = self.doc.page_size;
        let anim_data = self.doc.pages.get(idx).and_then(|p| {
            p.animation_sequence
                .as_ref()
                .map(|seq| (seq.clone(), p.animations.clone()))
        });
        if let Some((seq, map)) = anim_data {
            if let Some(page) = self.doc.pages.get_mut(idx) {
                self.animation_player
                    .init_page(seq, map, size, &mut page.elements);
            }
        }
    }

    fn import_enbx_path(&mut self, path: &std::path::Path) {
        log::info!("Importing ENBX: {}", path.display());
        self.set_status("Importing ENBX...");
        match enbx_importer::import_enbx(path, None) {
            Ok((doc, report)) => {
                self.doc = doc;
                self.history = History::new();
                self.interaction = InteractionState::new();
                self.multi_page = MultiPageState::from_doc(&self.doc);
                self.annotation.clear_screen();
                self.camera.offset = [self.doc.page_size[0] * 0.5, self.doc.page_size[1] * 0.5];
                self.init_page_animations();
                self.current_path = None;
                self.show_toast(&format!(
                    "Imported {} pages ({} failed), {} resources",
                    report.pages_ok, report.pages_failed, report.resources_extracted
                ));
                self.set_status("ENBX imported");
            }
            Err(e) => {
                self.show_toast(&format!("Import failed: {}", e));
                self.set_status("Import error");
            }
        }
    }

    fn save_action(&mut self) {
        if let Some(ref p) = self.current_path {
            match io::save_courseware(p, &self.doc) {
                Ok(()) => self.show_toast("  Saved"),
                Err(e) => self.show_toast(&format!("Save failed: {}", e)),
            }
        } else {
            self.export_action();
        }
    }

    fn export_action(&mut self) {
        let mut dialog = rfd::FileDialog::new().add_filter("Drafftink", &["drft"]);
        // 绑定父窗口句柄，确保对话框始终显示在主窗口之上
        if let Some(ref parent) = self.parent_window {
            dialog = dialog.set_parent(parent);
        }
        if let Some(p) = dialog.save_file() {
            match io::save_courseware(&p, &self.doc) {
                Ok(()) => {
                    self.current_path = Some(p);
                    self.show_toast("  Saved");
                }
                Err(e) => self.show_toast(&format!("Save error: {}", e)),
            }
        }
    }

    fn delete_selected(&mut self) {
        if self.interaction.selected_ids.is_empty() {
            return;
        }
        let ids = std::mem::take(&mut self.interaction.selected_ids);
        for page in &mut self.doc.pages {
            page.elements.retain(|e| !ids.contains(&e.id()));
        }
        self.doc.elements.retain(|e| !ids.contains(&e.id()));
        // Deletion is not undoable in this MVP
    }

    fn undo(&mut self) {
        self.history.undo();
    }

    fn redo(&mut self) {
        self.history.redo();
    }

    fn show_toast(&self, _msg: &str) {}
    fn toast_message_s(&mut self, _s: &str) {}

    fn set_status(&mut self, s: &str) {
        self.status_message = s.to_string();
    }

    // ================================================================
    // Menu bar
    // ================================================================

    fn render_menu(&mut self, ctx: &egui::Context) {
        let parent = self.parent_window;
        egui::TopBottomPanel::top("menu_bar")
            .min_height(18.0)
            .frame(egui::Frame::none().inner_margin(egui::Margin::symmetric(4.0, 0.0)))
            .show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        if ui.button("New").clicked() {
                            self.doc = CoursewareDoc::default();
                            ui.close_menu();
                        }
                        if ui.button("Open ENBX...").clicked() {
                            let mut dialog = rfd::FileDialog::new().add_filter("ENBX", &["enbx"]);
                            if let Some(ref p) = parent {
                                dialog = dialog.set_parent(p);
                            }
                            if let Some(picked) = dialog.pick_file() {
                                self.import_enbx_path(&picked);
                            }
                            ui.close_menu();
                        }
                        if ui.button("Save").clicked() {
                            self.save_action();
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Undo").clicked() {
                            self.undo();
                            ui.close_menu();
                        }
                        if ui.button("Redo").clicked() {
                            self.redo();
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Delete").clicked() {
                            self.delete_selected();
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Quit").clicked() {
                            std::process::exit(0);
                        }
                    });
                });
            });
    }

    // ================================================================
    // Canvas area
    // ================================================================

    fn render_canvas_area(&mut self, ui: &mut Ui) {
        let available = ui.available_size();
        let page_w = self.doc.page_size[0];
        let page_h = self.doc.page_size[1];

        // Inset canvas: scale to fit available area with 24px margins
        let margin = 24.0;
        let scale_x = (available.x - margin * 2.0) / page_w;
        let scale_y = (available.y - margin * 2.0) / page_h;
        let zoom = scale_x.min(scale_y).max(0.1);
        let canvas_w = page_w * zoom;
        let canvas_h = page_h * zoom;
        let offset_x = (available.x - canvas_w) * 0.5;
        let offset_y = (available.y - canvas_h) * 0.5;
        let canvas_rect = egui::Rect::from_min_size(
            ui.min_rect().min + egui::vec2(offset_x, offset_y),
            egui::vec2(canvas_w, canvas_h),
        );

        // White canvas background
        ui.painter().rect_filled(canvas_rect, 0.0, Color32::WHITE);
        ui.painter().rect_stroke(
            canvas_rect,
            0.0,
            egui::Stroke::new(1.0_f32, Color32::from_rgb(0xD0, 0xD0, 0xD0)),
        );

        self.camera.viewport = [canvas_w, canvas_h];
        self.camera.zoom = zoom;
        self.camera.offset = [page_w * 0.5, page_h * 0.5];

        // Sense only within canvas
        let response = ui.allocate_rect(canvas_rect, Sense::click_and_drag());

        if let Some(pos) = response.hover_pos() {
            let world = self.camera.screen_to_world(pos);
            self.interaction.cursor_screen = Some(pos);
            self.interaction.cursor_world = Some(world);
        }

        let is_edit = matches!(&self.current, ActiveBoard::Edit(_));
        if is_edit {
            if let ActiveBoard::Edit(ref mut board) = self.current {
                board.update(ui, &response, &self.camera);
            }
        } else {
            // Display mode: existing paths
            let annotation_active = matches!(
                self.annotation.current_tool,
                AnnotationTool::Pen
                    | AnnotationTool::Highlighter
                    | AnnotationTool::Eraser
                    | AnnotationTool::LaserPointer
            );
            if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let world = self.camera.screen_to_world(pos);
                    let idx = self.multi_page.current_page;
                    let elements = if let Some(page) = self.doc.pages.get_mut(idx) {
                        &mut page.elements[..]
                    } else {
                        &mut self.doc.elements[..]
                    };
                    self.animation_player.on_canvas_click(world, elements);
                }
            }
            if annotation_active {
                let ctx_clone = ui.ctx().clone();
                self.annotation.handle_input(&ctx_clone, &response);
            } else {
                self.handle_canvas_input(ui, &response);
            }
        }

        // Sync edit elements to doc for rendering
        if let ActiveBoard::Edit(ref board) = &self.current {
            let idx = self.multi_page.current_page;
            if let Some(page) = self.doc.pages.get_mut(idx) {
                page.elements.clone_from(&board.elements);
            }
        }

        let painter = ui.painter();
        render::render_canvas(
            painter,
            &self.doc,
            &self.camera,
            &self.interaction,
            self.multi_page.current_page,
        );
        self.annotation.paint(painter);
        self.annotation.paint_eraser_cursor(painter);

        if let ActiveBoard::Edit(ref board) = &self.current {
            board.render_overlay(painter, &self.camera);
        }
    }

    fn handle_canvas_input(&mut self, ui: &mut Ui, response: &egui::Response) {
        // Zoom
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta);
            let alt = ui.input(|i| i.modifiers.alt);
            if scroll.y != 0.0 {
                let zoom_factor = 1.0 + scroll.y * 0.001;
                if alt {
                    if let Some(pos) = response.hover_pos() {
                        self.camera.zoom_at(zoom_factor, pos);
                    }
                } else {
                    self.camera.zoom_center(zoom_factor);
                }
            }
        }
        let space_held = ui.input(|i| i.key_down(Key::Space));
        let panning = space_held || self.interaction.mode == ToolMode::Pan;
        if panning && response.dragged_by(egui::PointerButton::Primary) {
            let delta = response.drag_delta();
            self.camera.pan_screen([-delta.x, -delta.y]);
            return;
        }
        match self.interaction.mode {
            ToolMode::Select => self.handle_select_input(ui, response),
            ToolMode::DrawShape(_) => self.handle_draw_shape_input(ui, response),
            ToolMode::DrawPath => self.handle_draw_path_input(ui, response),
            ToolMode::Text => self.handle_text_input(ui, response),
            ToolMode::Image => self.handle_image_input(),
            _ => {}
        }
    }

    fn handle_select_input(&mut self, ui: &mut Ui, response: &egui::Response) {
        let _ = (ui, response);
    }

    fn handle_draw_shape_input(&mut self, ui: &mut Ui, response: &egui::Response) {
        let _ = (ui, response);
    }

    fn handle_draw_path_input(&mut self, ui: &mut Ui, response: &egui::Response) {
        let _ = (ui, response);
    }

    fn handle_text_input(&mut self, ui: &mut Ui, response: &egui::Response) {
        let _ = (ui, response);
    }

    fn handle_image_input(&mut self) {}

    // ================================================================
    // Bottom toolbar
    // ================================================================

    fn render_bottom_toolbar(&mut self, ctx: &egui::Context) {
        egui::Area::new("bottom_toolbar".into())
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -16.0])
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(0x1E, 0x1E, 0x1E))
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        egui::Color32::from_rgb(0x33, 0x33, 0x33),
                    ))
                    .rounding(egui::Rounding::same(12.0))
                    .shadow(egui::epaint::Shadow {
                        offset: egui::Vec2::new(0.0, 2.0),
                        blur: 8.0,
                        spread: 0.0,
                        color: egui::Color32::from_rgba_unmultiplied(0, 0, 0, 40),
                    })
                    .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let select_btn = make_tool_button(ui, "选择", false);
                            if select_btn.clicked() {
                                self.annotation.set_tool(AnnotationTool::Pen);
                                self.interaction.mode = ToolMode::Select;
                            }
                            ui.add_space(4.0);
                            let pen_active =
                                matches!(self.annotation.current_tool, AnnotationTool::Pen);
                            if make_tool_button(ui, "笔", pen_active).clicked() {
                                self.annotation.set_tool(AnnotationTool::Pen);
                                self.show_color_panel = !self.show_color_panel;
                                self.show_more_panel = false;
                            }
                            ui.add_space(4.0);
                            let eraser_active =
                                matches!(self.annotation.current_tool, AnnotationTool::Eraser);
                            if make_tool_button(ui, "橡皮", eraser_active).clicked() {
                                self.annotation.set_tool(AnnotationTool::Eraser);
                                self.show_color_panel = false;
                                self.show_more_panel = false;
                            }
                            ui.add_space(2.0);
                            if make_tool_button(ui, "撤销", false).clicked() {
                                self.annotation.undo();
                            }
                            ui.add_space(2.0);
                            if make_tool_button(ui, "恢复", false).clicked() {
                                self.set_status("Redo not yet supported");
                            }
                            ui.add_space(2.0);
                            let clear_active =
                                matches!(self.annotation.current_tool, AnnotationTool::ClearScreen);
                            if make_tool_button(ui, "清屏", clear_active).clicked() {
                                self.annotation.set_tool(AnnotationTool::ClearScreen);
                                self.annotation.clear_screen();
                                self.multi_page.clear_page_annotations();
                            }
                            ui.add_space(2.0);
                            if make_tool_button(ui, "更多", self.show_more_panel).clicked() {
                                self.show_more_panel = !self.show_more_panel;
                                self.show_color_panel = false;
                            }
                            ui.add_space(6.0);
                            if make_tool_button(ui, "视频展台", false).clicked() {
                                self.set_status("Visualizer (placeholder)");
                            }
                            if make_tool_button(ui, "录制胶囊", false).clicked() {
                                self.set_status("Recording (placeholder)");
                            }
                            if make_tool_button(ui, "手机投屏", false).clicked() {
                                self.set_status("Phone cast (placeholder)");
                            }
                        });
                    });
            });
    }

    fn render_color_panel(&mut self, ctx: &egui::Context) {
        if !self.show_color_panel {
            return;
        }
        let open = self.show_color_panel;
        egui::Window::new("color_panel")
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -76.0])
            .frame(
                egui::Frame::window(&ctx.style())
                    .rounding(egui::Rounding::same(8.0))
                    .inner_margin(egui::Margin::same(12.0)),
            )
            .show(ctx, |ui| {
                ui.label("Color picker placeholder");
            });
        self.show_color_panel = open;
    }

    fn render_eraser_panel(&mut self, ctx: &egui::Context) {
        if !matches!(self.annotation.current_tool, AnnotationTool::Eraser)
            || !self.annotation.show_eraser_panel
        {
            return;
        }
        egui::Window::new("eraser_panel")
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -100.0])
            .show(ctx, |ui| {
                ui.label("橡皮大小");
                ui.horizontal(|ui| {
                    for &s in &[4.0, 8.0, 16.0] {
                        if ui.button(format!("{:.0}", s)).clicked() {
                            self.annotation.eraser_size = s;
                        }
                    }
                });
            });
    }

    fn render_more_panel(&mut self, ctx: &egui::Context) {
        if !self.show_more_panel {
            return;
        }
        egui::Window::new("more_panel")
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -76.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("📊 课堂互动").clicked() {
                        self.show_more_panel = false;
                        if self.quiz_ui_state.is_none() {
                            self.start_quiz();
                        }
                        self.show_quiz_panel = !self.show_quiz_panel;
                    }
                });
                ui.separator();
                ui.label("更多工具 (即将推出)");
            });
    }

    // ── Quiz 系统 ──────────────────────────────────────────────────

    /// 启动 Quiz 系统（后台 tokio 运行时）
    fn start_quiz(&mut self) {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                log::error!("[app] 无法创建 tokio 运行时: {}", e);
                return;
            }
        };

        let config = QuizConfig::default();
        let quiz = match rt.block_on(drafftink_quiz::QuizRuntime::start(config)) {
            Ok(q) => q,
            Err(e) => {
                log::error!("[app] Quiz 启动失败: {}", e);
                return;
            }
        };

        self.quiz_ui_state = Some(quiz.ui_state);
        self.quiz_session_tx = Some(quiz.session_tx);
        self.quiz_runtime = Some(rt);

        log::info!("[app] Quiz 系统已启动");
    }

    /// 渲染 Quiz 面板（全屏覆盖模式）
    fn render_quiz_panel(&mut self, ctx: &egui::Context) {
        if !self.show_quiz_panel {
            return;
        }

        let ui_state = match &self.quiz_ui_state {
            Some(s) => s.clone(),
            None => return,
        };
        let session_tx = match &self.quiz_session_tx {
            Some(tx) => tx.clone(),
            None => return,
        };

        // 全屏暗色遮罩
        egui::Area::new(egui::Id::new("quiz_overlay"))
            .fixed_pos(egui::pos2(0.0, 0.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let screen_rect = ctx.screen_rect();
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(screen_rect), |ui| {
                    // 关闭按钮
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                            if ui.button("✕ 退出互动").clicked() {
                                self.show_quiz_panel = false;
                            }
                        });
                    });
                    // Quiz 主面板
                    self.quiz_panel.ui(ui, &ui_state, &session_tx);
                });
            });
    }

    /// 停止 Quiz 系统
    #[allow(dead_code)]
    fn stop_quiz(&mut self) {
        self.show_quiz_panel = false;
        self.quiz_ui_state = None;
        self.quiz_session_tx = None;
        if let Some(rt) = self.quiz_runtime.take() {
            rt.shutdown_background();
        }
        log::info!("[app] Quiz 系统已停止");
    }

    fn render_properties(&self, _ctx: &egui::Context) {}

    fn render_export_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_export_dialog {
            return;
        }
        egui::Window::new("Export PNG").show(ctx, |ui| {
            ui.label("Width:");
            ui.add(egui::DragValue::new(&mut self.export_width).range(1..=8192));
            ui.label("Height:");
            ui.add(egui::DragValue::new(&mut self.export_height).range(1..=8192));
            if ui.button("Export").clicked() {
                self.show_export_dialog = false;
            }
        });
    }

    fn render_toast(&self, _ctx: &egui::Context) {}

    fn unload_enbx_plugin(&mut self) {
        self.animation_player.shutdown();
        if self.enbx_plugin.take().is_some() {
            self.show_toast("ENBX plugin unloaded");
            self.set_status("ENBX plugin unloaded");
        }
    }
}

// ==========================================================================
// eframe::App
// ==========================================================================

impl eframe::App for SeewoClassApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // ── 强制深色主题：覆盖 Windows 系统主题偏好，防止浅色模式下文字不可见 ──
        ctx.set_theme(egui::ThemePreference::Dark);

        // ── 更新父窗口句柄（用于文件对话框绑定，防止遮挡）──
        if let Ok(handle) = frame.window_handle() {
            self.parent_window = Some(ParentWindow(handle.as_raw()));
        }

        // Ctrl+E toggle
        let ctrl_e = ctx.input(|i| i.key_pressed(egui::Key::E) && i.modifiers.ctrl);
        if ctrl_e {
            log::info!("Ctrl+E — toggling mode");
            self.toggle_mode();
        } else {
            // Diagnostic
            let any_ctrl = ctx.input(|i| {
                i.modifiers.ctrl
                    && (i.key_pressed(egui::Key::Z)
                        || i.key_pressed(egui::Key::Y)
                        || i.key_pressed(egui::Key::S)
                        || i.key_pressed(egui::Key::A)
                        || i.key_pressed(egui::Key::E))
            });
            if any_ctrl {
                log::info!("Ctrl+ key pressed but not E");
            }
        }

        self.apply_style(ctx);
        self.handle_shortcuts(ctx);
        self.handle_drop(ctx);
        self.handle_eraser_scroll(ctx);

        // ── Animation tick (Display mode) ──────────────────────────
        let now = std::time::Instant::now();
        {
            let idx = self.multi_page.current_page;
            let elements = if let Some(page) = self.doc.pages.get_mut(idx) {
                &mut page.elements[..]
            } else {
                &mut self.doc.elements[..]
            };
            self.animation_player.update(now, elements);
        }
        if self.animation_player.is_active() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }

        // ── Layout ─────────────────────────────────────────────────
        let is_edit = matches!(&self.current, ActiveBoard::Edit(_));
        let parent = self.parent_window;

        if is_edit {
            // ── Edit mode: full editor UI ──────────────────────────
            // Top toolbar
            egui::TopBottomPanel::top("editor_toolbar")
                .min_height(36.0)
                .frame(
                    egui::Frame::none()
                        .fill(TOOLBAR_BG)
                        .inner_margin(egui::Margin::symmetric(12.0, 4.0)),
                )
                .show(ctx, |ui| {
                    let v = ui.visuals_mut();
                    v.widgets.noninteractive.fg_stroke.color = Color32::from_rgb(0xE0, 0xE0, 0xE0);
                    v.widgets.inactive.fg_stroke.color = Color32::from_rgb(0xE0, 0xE0, 0xE0);
                    ui.horizontal(|ui| {
                        ui.menu_button(" 文件", |ui| {
                            if ui.button("New").clicked() {
                                ui.close_menu();
                            }
                            if ui.button("Open ENBX...").clicked() {
                                let mut dialog =
                                    rfd::FileDialog::new().add_filter("ENBX", &["enbx"]);
                                if let Some(ref p) = parent {
                                    dialog = dialog.set_parent(p);
                                }
                                if let Some(picked) = dialog.pick_file() {
                                    self.import_enbx_path(&picked);
                                }
                                ui.close_menu();
                            }
                            if ui.button("Save").clicked() {
                                self.save_action();
                                ui.close_menu();
                            }
                        });
                        label_btn(ui, " 同步", TOOLBAR_BG).clicked();
                        if label_btn(ui, " 撤销", TOOLBAR_BG).clicked() {
                            self.undo();
                        }
                        if label_btn(ui, " 恢复", TOOLBAR_BG).clicked() {
                            self.redo();
                        }
                        ui.separator();
                        if label_btn(ui, "T 文本", TOOLBAR_BG).clicked() {
                            self.interaction.mode = ToolMode::Text;
                        }
                        if label_btn(ui, " 形状", TOOLBAR_BG).clicked() {
                            self.interaction.mode = ToolMode::DrawShape(ShapeType::Rectangle);
                        }
                        label_btn(ui, " 多媒体", TOOLBAR_BG).clicked();
                        label_btn(ui, " 表格", TOOLBAR_BG).clicked();
                        label_btn(ui, " 课堂活动", TOOLBAR_BG).clicked();
                        label_btn(ui, " 思维导图", TOOLBAR_BG).clicked();
                        label_btn(ui, " 学科工具", TOOLBAR_BG).clicked();
                        ui.separator();
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            label_btn(ui, " 分享", TOOLBAR_BG).clicked();
                            if ui
                                .add_sized(
                                    [88.0, 24.0],
                                    egui::Button::new(
                                        egui::RichText::new(" 授课").color(Color32::WHITE).strong(),
                                    )
                                    .fill(Color32::from_rgb(0x07, 0xC1, 0x60))
                                    .rounding(egui::Rounding::same(6.0)),
                                )
                                .clicked()
                            {
                                self.enter_display_mode();
                            }
                        });
                    });
                });

            // Left page list (forced visible)
            egui::SidePanel::left("page_sidebar")
                .resizable(false)
                .default_width(80.0)
                .show(ctx, |ui| {
                    ui.visuals_mut().widgets.noninteractive.bg_fill = SIDEBAR_BG;
                    ui.add_space(2.0);
                    if ui.button("+\n新建").clicked() {
                        // TODO: add new page
                    }
                    ui.add_space(8.0);
                    for i in 0..self.doc.pages.len().max(2) {
                        let active = i == self.multi_page.current_page;
                        let stroke_col = if active { PAGE_ACTIVE } else { PAGE_INACTIVE };
                        let f = egui::Frame::none()
                            .fill(Color32::WHITE)
                            .stroke(egui::Stroke::new(
                                if active { 2.0_f32 } else { 1.0_f32 },
                                stroke_col,
                            ))
                            .inner_margin(egui::Margin::same(2.0));
                        let resp = f.show(ui, |ui| {
                            ui.set_width(60.0);
                            ui.set_height(32.0);
                            ui.centered_and_justified(|ui| {
                                ui.label(format!("{}", i + 1));
                            });
                        });
                        if resp.response.clicked() && i != self.multi_page.current_page {
                            self.multi_page.save_annotations(&self.annotation);
                            self.multi_page.current_page = i;
                            self.annotation.clear_screen();
                            self.multi_page.load_annotations(&mut self.annotation);
                            self.init_page_animations();
                        }
                        ui.add_space(6.0);
                    }
                });

            // ── Right Inspector (declared BEFORE CentralPanel!) ────
            egui::SidePanel::right("inspector")
                .min_width(250.0)
                .max_width(250.0)
                .frame(
                    egui::Frame::none()
                        .fill(SIDEBAR_BG)
                        .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(0xC0, 0xC0, 0xC0)))
                        .inner_margin(egui::Margin::same(10.0)),
                )
                .show(ctx, |ui| {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("布局与背景").size(14.0).strong());
                    ui.add_space(8.0);

                    // Card 1: page settings
                    ui.group(|ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(egui::RichText::new("页面设置").size(12.0).strong());
                        ui.add_space(4.0);
                        // White thumbnail
                        let thumb =
                            egui::Rect::from_min_size(ui.cursor().min, egui::vec2(80.0, 48.0));
                        ui.painter().rect_filled(thumb, 2.0, Color32::WHITE);
                        ui.painter().rect_stroke(
                            thumb,
                            2.0,
                            egui::Stroke::new(1.0_f32, Color32::from_rgb(0xD0, 0xD0, 0xD0)),
                        );
                        ui.advance_cursor_after_rect(thumb);
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.button("空白").clicked();
                            if ui.button("更改布局").clicked() {}
                        });
                    });
                    ui.add_space(8.0);

                    // Card 2: background settings
                    ui.group(|ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(egui::RichText::new("背景").size(12.0).strong());
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let mut color = Color32::WHITE;
                            ui.color_edit_button_srgba(&mut color);
                            let _ = ui.button("更多背景");
                            let _ = ui.button("本地图片");
                        });
                    });
                    ui.add_space(8.0);

                    // Theme button
                    ui.button("应用主题").clicked();

                    ui.add_space(12.0);
                    ui.separator();
                    ui.label(egui::RichText::new("属性").size(14.0).strong());
                    ui.add_space(4.0);
                    if let ActiveBoard::Edit(ref board) = &self.current {
                        if board.selected.len() == 1 {
                            ui.label("选中元素属性");
                        } else if board.selected.len() > 1 {
                            ui.label(format!("{} 个元素已选中", board.selected.len()));
                        } else {
                            ui.label(
                                egui::RichText::new("未选中元素")
                                    .color(Color32::from_rgb(0x99, 0x99, 0x99)),
                            );
                        }
                    }
                });

            // ── Central canvas (declared LAST — does NOT eat side panels) ──
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(CANVAS_BG))
                .show(ctx, |ui| {
                    self.render_canvas_area(ui);
                });

            // Bottom floating UI
            self.render_bottom_toolbar(ctx);
            self.render_color_panel(ctx);
            self.render_eraser_panel(ctx);
            self.render_more_panel(ctx);
        } else {
            // ── Display mode: minimal layout ───────────────────────
            // Mode indicator (declared BEFORE CentralPanel so it's visible)
            egui::TopBottomPanel::top("mode_indicator")
                .min_height(44.0)
                .frame(
                    egui::Frame::none()
                        .fill(Color32::from_rgb(0xFF, 0xFF, 0xFF))
                        .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(0xCC, 0xCC, 0xCC)))
                        .inner_margin(egui::Margin::symmetric(12.0, 6.0)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("当前模式:")
                                .color(Color32::from_rgb(0x66, 0x66, 0x66))
                                .size(13.0),
                        );
                        if ui
                            .add_sized(
                                [90.0, 30.0],
                                egui::Button::new(
                                    egui::RichText::new("DISPLAY")
                                        .color(Color32::WHITE)
                                        .strong()
                                        .size(14.0),
                                )
                                .fill(Color32::from_rgb(0x66, 0x99, 0xCC))
                                .rounding(egui::Rounding::same(6.0)),
                            )
                            .clicked()
                        {
                            self.toggle_mode();
                        }
                        ui.add_space(12.0);
                        if ui
                            .add_sized(
                                [110.0, 30.0],
                                egui::Button::new(
                                    egui::RichText::new(" 切到 EDIT")
                                        .color(Color32::from_rgb(0x07, 0xC1, 0x60))
                                        .strong()
                                        .size(13.0),
                                )
                                .fill(Color32::TRANSPARENT)
                                .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(0x07, 0xC1, 0x60)))
                                .rounding(egui::Rounding::same(6.0)),
                            )
                            .clicked()
                        {
                            self.toggle_mode();
                        }
                        ui.add_space(20.0);
                        ui.label(
                            egui::RichText::new("快捷键: Ctrl+E")
                                .color(Color32::from_rgb(0x99, 0x99, 0x99))
                                .size(11.0),
                        );
                    });
                });
            egui::CentralPanel::default().show(ctx, |ui| {
                self.render_canvas_area(ui);
            });
            self.render_bottom_toolbar(ctx);
            self.render_color_panel(ctx);
            self.render_eraser_panel(ctx);
            self.render_more_panel(ctx);
        }

        // Common
        self.render_export_dialog(ctx);
        self.render_toast(ctx);
        self.render_quiz_panel(ctx);

        #[cfg(debug_assertions)]
        self.animation_player.debug_ui(ctx);
    }
}

// ==========================================================================
// Helpers
// ==========================================================================

fn make_tool_button(ui: &mut Ui, label: &str, active: bool) -> egui::Response {
    let fill = if active {
        Color32::from_rgb(0x4A, 0x6E, 0xC0)
    } else {
        Color32::TRANSPARENT
    };
    let text_color = if active {
        Color32::WHITE
    } else {
        Color32::from_rgb(0xE0, 0xE0, 0xE0)
    };
    let text = egui::RichText::new(label).size(13.0).color(text_color);
    let btn = egui::Button::new(text)
        .min_size(egui::Vec2::new(56.0, 32.0))
        .fill(fill)
        .rounding(egui::Rounding::same(8.0));
    ui.add(btn)
}

fn label_btn(ui: &mut Ui, label: &str, bg: Color32) -> egui::Response {
    let btn = egui::Button::new(
        egui::RichText::new(label)
            .size(12.0)
            .color(Color32::from_rgb(0xE0, 0xE0, 0xE0)),
    )
    .fill(bg)
    .frame(false)
    .min_size(egui::vec2(0.0, 24.0));
    ui.add(btn)
}

// ==========================================================================
// Plugin host callbacks (C ABI)
// ==========================================================================

unsafe fn read_plugin_str(ptr: *const u8, len: u32) -> String {
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    String::from_utf8_lossy(slice).into_owned()
}

unsafe extern "C" fn plugin_log(level: u8, msg: *const u8, len: u32) {
    let msg = unsafe { read_plugin_str(msg, len) };
    let lvl = match level {
        1 => log::Level::Error,
        2 => log::Level::Warn,
        3 => log::Level::Info,
        4 => log::Level::Debug,
        _ => log::Level::Trace,
    };
    log::log!(lvl, "[plugin] {}", msg);
}
