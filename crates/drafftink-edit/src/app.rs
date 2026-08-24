//! Drafftink Edit — editor application logic.

use drafftink_core::board::{EditBoard, Snapshot};
use drafftink_core::document;
use drafftink_core::model::{CoursewareDoc, ShapeKind, ShapeType};
use drafftink_core::Camera;
use egui::{Color32, Sense, Ui};
use raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, WindowHandle};
use std::sync::{Arc, Mutex};
use drafftink_core::integration::SharedAppContext;

use crate::annotation::AnnotationState;
use crate::interaction::{InteractionState, ToolMode};
use crate::multi_page::MultiPageState;

/// 顶边栏形状选择器的展示标签（含符号前缀，便于一眼区分形状种类）。
fn shape_kind_label(k: ShapeKind) -> &'static str {
    match k {
        ShapeKind::Circle => "○ 圆形",
        ShapeKind::Square => "□ 正方形",
        ShapeKind::Rectangle => "▭ 长方形",
        ShapeKind::RoundedRect => "▢ 圆角矩形",
        ShapeKind::Parenthesis => "( 小括号",
        ShapeKind::Bracket => "[ 中括号",
        ShapeKind::Brace => "{ 大括号",
        ShapeKind::Arrow => "→ 箭头",
        ShapeKind::DoubleArrow => "⇌ 双箭头",
        // 虚拟教具提交产物：不出现在顶边栏形状选择器里（由教具菜单直接激活），
        // 仅需保证穷举匹配完整。
        ShapeKind::Line => "／ 直线",
        ShapeKind::Arc => "⌒ 圆弧",
        ShapeKind::Sector => "扇形",
        ShapeKind::Angle => "∠ 角",
        ShapeKind::Polygon { .. } => "⬡ 正多边形",
        ShapeKind::NumberLine(_) => "📏 数轴",
    }
}

/// 顶边栏「📐 教具」下拉菜单可选的虚拟教具种类。
///
/// 仅是「老师选了哪个教具」这一 UI 请求信号：`EditApp` 置位 `tool_request`
/// 后由宿主（`drafftink-desktop`）消费并创建对应的交互教具对象，备课端自身
/// 不实现任何教具交互逻辑（遵守「不改 edit/display 核心逻辑」红线）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeachingToolKind {
    /// 圆规（C）：拖拽定圆心与半径，可提交圆 / 弧 / 扇形。
    Compass,
    /// 三角尺 30°-60°-90°（T）。
    SetSquare30,
    /// 三角尺 45°-45°-90°。
    SetSquare45,
    /// 量角器（P）：0°-180° 测角 / 画角 / 画弧。
    Protractor,
    /// 直尺（R）：拖两端调长度、拖中间平移，Shift 吸附水平/垂直。
    Ruler,
    /// 正多边形（3–12 边）：点中心 → 拖半径 → 预览旋转 → 提交。
    Polygon(u8),
    /// 函数绘图（F）：输入表达式 → 画坐标系 + 曲线 → 提交。
    FunctionPlot,
    /// 数轴（N）：点击定起点 → 拖拽定终点（Shift 吸附水平/垂直）→ 松开提交。
    NumberLine,
}

impl TeachingToolKind {
    /// 菜单项展示标签。
    pub fn label(&self) -> &'static str {
        match self {
            TeachingToolKind::Compass => "🧭 圆规 (C)",
            TeachingToolKind::SetSquare30 => "📐 三角尺 30°-60°-90° (T)",
            TeachingToolKind::SetSquare45 => "📐 三角尺 45°-45°-90°",
            TeachingToolKind::Protractor => "📏 量角器 (P)",
            TeachingToolKind::Ruler => "📏 直尺 (R)",
            TeachingToolKind::Polygon(n) => match n {
                3 => "▲ 正三角形 (3边)",
                4 => "◆ 正方形 (4边)",
                5 => "⬠ 正五边形 (5边)",
                6 => "⬡ 正六边形 (6边)",
                7 => "⬡ 正七边形 (7边)",
                8 => "⬡ 正八边形 (8边)",
                9 => "⬡ 正九边形 (9边)",
                10 => "⬡ 正十边形 (10边)",
                11 => "⬡ 正十一边形 (11边)",
                12 => "⬡ 正十二边形 (12边)",
                _ => "⬡ 正多边形",
            },
            TeachingToolKind::FunctionPlot => "📈 函数绘图 (F)",
            TeachingToolKind::NumberLine => "📏 数轴 (N)",
        }
    }
}

// ── Colors ─────────────────────────────────────────────────────────────────
const TOOLBAR_BG: Color32     = Color32::from_rgb(0x3C, 0x3C, 0x3C);
const SIDEBAR_BG: Color32     = Color32::from_rgb(0xF5, 0xF5, 0xF5);
const CANVAS_BG: Color32      = Color32::from_rgb(0xE0, 0xE0, 0xE0);
const PAGE_ACTIVE: Color32    = Color32::from_rgb(0x00, 0xC8, 0x00);
const PAGE_INACTIVE: Color32  = Color32::from_rgb(0xD0, 0xD0, 0xD0);

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

pub struct EditApp {
    pub doc: CoursewareDoc,
    pub camera: Camera,
    pub interaction: InteractionState,
    pub annotation: AnnotationState,
    pub multi_page: MultiPageState,
    pub board: EditBoard,
    /// 父窗口句柄（用于文件对话框 set_parent，防止遮挡）
    parent_window: Option<ParentWindow>,
    /// 备授一体共享上下文（由宿主注入）。仅用于上层状态传递，不进入核心逻辑。
    pub shared: Option<Arc<Mutex<SharedAppContext>>>,
    /// 备课端「授课」按钮置位，由宿主读取后切到授课模式（替代原 spawn 子进程）。
    pub teach_requested: bool,
    /// 备课端「保存」按钮置位，由宿主读取后把批注层落盘（仅在显式保存时触发）。
    pub save_requested: bool,
    /// 备课端「🎬 多媒体」按钮置位，由宿主读取后弹出视频文件选择框并插入视频元素。
    pub media_pick_requested: bool,
    /// 备课端「🖼 图片」按钮置位，由宿主读取后弹出图片文件选择框并插入图片元素。
    pub image_pick_requested: bool,
    /// 备课端「🎵 音频」按钮置位，由宿主读取后弹出音频文件选择框并插入音频控制条。
    pub audio_pick_requested: bool,
    /// 备课端「🔷 形状」选择器当前选中的形状种类（顶边栏 ComboBox 维护）。
    pub selected_shape: ShapeKind,
    /// 备课端「🔷 形状」→「➕ 插入」按钮置位，由宿主读取后在画布中心插入对应形状叠加层。
    pub shape_insert_requested: Option<ShapeKind>,
    /// 备课端「💾 保存」按钮置位，由宿主读取后弹出 .enbx 保存对话框并导出课件。
    pub enbx_save_requested: bool,
    /// 备课端「T 文本」按钮置位，由宿主读取后在画布中心插入默认文本框并选中。
    pub text_insert_requested: Option<()>,
    /// 备课端「📐 教具」下拉选中的教具，由宿主读取后激活对应虚拟教具覆盖层。
    pub tool_requested: Option<TeachingToolKind>,
    /// 画布在窗口中的屏幕坐标偏移（含顶栏/侧栏高度），供宿主的视频叠加层把世界
    /// 坐标换算回屏幕坐标，使插入的视频锚定到画布上的正确位置。
    pub canvas_offset: [f32; 2],
}

impl Default for EditApp {
    fn default() -> Self {
        Self {
            doc: CoursewareDoc::default(),
            camera: Camera::default(),
            interaction: InteractionState::new(),
            annotation: AnnotationState::new(),
            multi_page: MultiPageState::new(),
            board: EditBoard::default(),
            parent_window: None,
            shared: None,
            teach_requested: false,
            save_requested: false,
            media_pick_requested: false,
            image_pick_requested: false,
            audio_pick_requested: false,
            selected_shape: ShapeKind::Circle,
            shape_insert_requested: None,
            enbx_save_requested: false,
            text_insert_requested: None,
            tool_requested: None,
            canvas_offset: [0.0, 0.0],
        }
    }
}

impl EditApp {
    /// Load a .drft file. Also accepts .enbx for backward compatibility.
    pub fn open_file(&mut self, path: &std::path::Path) {
        log::info!("Opening: {}", path.display());
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match ext.as_str() {
            "drft" => match document::load_document(path) {
                Ok(doc) => {
                    self.setup_doc(doc);
                    log::info!("DRFT loaded: {} pages", self.doc.pages.len());
                }
                Err(e) => log::error!("Failed to load DRFT: {e}"),
            },
            "enbx" => match enbx_importer::import_enbx(path, None) {
                Ok((doc, report)) => {
                    self.setup_doc(doc);
                    log::info!("ENBX imported: {} pages", report.pages_ok);
                }
                Err(e) => log::error!("ENBX import failed: {e}"),
            },
            _ => log::warn!("Unknown file type: {}", ext),
        }
        // 记录当前课件路径到共享上下文，授课模式可直接读取（上层状态传递）。
        if let Some(ref s) = self.shared {
            if let Ok(mut g) = s.lock() {
                g.current_doc_path = Some(path.to_path_buf());
            }
        }
    }

    fn setup_doc(&mut self, doc: CoursewareDoc) {
        self.doc = doc;
        self.interaction = InteractionState::new();
        self.multi_page = MultiPageState::from_doc(&self.doc);
        self.annotation.clear_screen();
        self.camera.offset = [self.doc.page_size[0] * 0.5, self.doc.page_size[1] * 0.5];
        let snap = Snapshot::from_doc(&self.doc, 0);
        self.board.load_snapshot(&snap);
    }

    pub fn save_drft(&mut self, path: &std::path::Path) {
        // Flush board state to doc
        let idx = self.multi_page.current_page;
        if let Some(page) = self.doc.pages.get_mut(idx) {
            page.elements.clone_from(&self.board.elements);
        }
        match document::save_document(path, &self.doc) {
            Ok(n) => log::info!("Saved {}: {} bytes payload", path.display(), n),
            Err(e) => log::error!("Save failed: {e}"),
        }
    }

    pub fn import_enbx(&mut self, path: &std::path::Path) {
        self.open_file(path);
    }

    /// 注入备授一体共享上下文（宿主在构造后调用）。
    pub fn set_shared(&mut self, ctx: Arc<Mutex<SharedAppContext>>) {
        self.shared = Some(ctx);
    }

    /// 将当前页的板书/编辑落盘到 `doc`，供切换到授课模式时加载最新内容。
    /// 不改变任何核心编辑逻辑，仅做状态同步（与 `save_drft` 的刷新一致）。
    pub fn flush_to_doc(&mut self) {
        let idx = self.multi_page.current_page;
        if let Some(page) = self.doc.pages.get_mut(idx) {
            page.elements.clone_from(&self.board.elements);
        }
    }

    // ── 备授一体：批注双向同步（仅批注层，绝不触碰 elements） ──

    /// 导出当前页的备课批注，转换为 `drafftink_core` 中性格式（无 egui 依赖），
    /// 供授课端加载。仅读取 `multi_page` 的批注层，不修改任何内容层。
    pub fn export_current_annotations(&mut self) -> Vec<document::StrokeData> {
        // 先把 live 笔迹落盘到当前页批注层
        self.multi_page.save_annotations(&self.annotation);
        let page = self.multi_page.current_page;
        let strokes = self
            .multi_page
            .pages
            .get(page)
            .map(|p| p.annotations.clone())
            .unwrap_or_default();
        strokes.iter().map(Self::edit_stroke_to_core).collect()
    }

    /// 把授课端回传的中性批注合并进当前页批注层与 live 笔迹。
    /// 仅写入 `annotations_data` 对应层，绝不触碰 `elements`（学生作答快照）。
    pub fn import_current_annotations(&mut self, strokes: Vec<document::StrokeData>) {
        let edit_strokes: Vec<crate::annotation::StrokeData> =
            strokes.iter().map(Self::core_stroke_to_edit).collect();
        if let Some(p) = self.multi_page.pages.get_mut(self.multi_page.current_page) {
            p.annotations = edit_strokes.clone();
        }
        self.annotation.set_strokes(edit_strokes);
    }

    /// 把授课端可能修改的几何 / 内容元素同步回备课端（仅内容层，不影响批注层）。
    /// 复用 `MultiPageState` 既有结构，不修改核心序列化逻辑。
    pub fn sync_doc_elements_from(&mut self, other: &CoursewareDoc) {
        let n = self.multi_page.pages.len().min(other.pages.len());
        for i in 0..n {
            let els = other.pages[i].elements.clone();
            if let Some(p) = self.multi_page.pages.get_mut(i) {
                p.elements = els.clone();
            }
            if let Some(p) = self.doc.pages.get_mut(i) {
                p.elements = els;
            }
        }
    }

    /// 用户主动保存时调用：把合并后的批注层（及内容层）写回 drftx。
    /// 仅在显式保存触发，避免频繁 IO。复用既有 `sync_to_doc` + `save_drft`。
    /// 仅写入 `annotations_data`（批注层），不修改学生作答快照。
    pub fn flush_annotations_to_doc(&mut self) {
        self.multi_page.sync_to_doc(&mut self.doc);
        let path = self
            .shared
            .as_ref()
            .and_then(|s| s.lock().ok())
            .and_then(|g| g.current_doc_path.clone());
        if let Some(p) = path {
            self.save_drft(&p);
        } else {
            log::warn!("[edit] flush_annotations_to_doc: 无课件路径，跳过落盘");
        }
    }

    /// 备课端 `StrokeData` → 核心中性 `StrokeData`（无 egui 依赖）。
    fn edit_stroke_to_core(s: &crate::annotation::StrokeData) -> document::StrokeData {
        let mut points = Vec::new();
        for seg in &s.segments {
            for p in seg {
                points.push([p.x, p.y]);
            }
        }
        document::StrokeData {
            points,
            color: [s.color.r(), s.color.g(), s.color.b(), s.color.a()],
            thickness: s.thickness,
            tool: match s.tool_type {
                crate::annotation::AnnotationTool::Pen => 0,
                crate::annotation::AnnotationTool::Highlighter => 1,
                crate::annotation::AnnotationTool::Eraser => 2,
                _ => 0,
            },
        }
    }

    /// 核心中性 `StrokeData` → 备课端 `StrokeData`。
    fn core_stroke_to_edit(s: &document::StrokeData) -> crate::annotation::StrokeData {
        let segments = vec![s
            .points
            .iter()
            .map(|p| egui::Pos2::new(p[0], p[1]))
            .collect::<Vec<_>>()];
        let color =
            egui::Color32::from_rgba_unmultiplied(s.color[0], s.color[1], s.color[2], s.color[3]);
        let tool_type = match s.tool {
            0 => crate::annotation::AnnotationTool::Pen,
            1 => crate::annotation::AnnotationTool::Highlighter,
            2 => crate::annotation::AnnotationTool::Eraser,
            _ => crate::annotation::AnnotationTool::Pen,
        };
        crate::annotation::StrokeData {
            segments,
            color,
            thickness: s.thickness,
            tool_type,
        }
    }

    fn render_canvas_area(&mut self, ui: &mut Ui) {
        // SAFETY: clamp everything to CentralPanel bounds first
        let available = ui.available_size();
        let panel_rect = ui.max_rect();
        if available.x < 10.0 || available.y < 10.0 {
            // Panel too small, skip
            return;
        }

        let page_w = self.doc.page_size[0];
        let page_h = self.doc.page_size[1];

        let margin = 24.0;
        let scale_x = (available.x - margin * 2.0) / page_w;
        let scale_y = (available.y - margin * 2.0) / page_h;
        let zoom = scale_x.min(scale_y).max(0.1);
        let canvas_w = page_w * zoom;
        let canvas_h = page_h * zoom;
        let offset_x = (available.x - canvas_w) * 0.5;
        let offset_y = (available.y - canvas_h) * 0.5;

        // CRITICAL: anchor canvas_rect to panel_rect.min, not ui.min_rect()
        let canvas_rect = egui::Rect::from_min_size(
            panel_rect.min + egui::vec2(offset_x, offset_y),
            egui::vec2(canvas_w, canvas_h),
        );

        // 把画布在窗口中的屏幕坐标偏移暴露给宿主，供视频叠加层做世界→屏幕换算。
        self.canvas_offset = [canvas_rect.min.x, canvas_rect.min.y];

        // Clip painter to panel rect so we never paint over neighbouring Panels
        let painter = ui.painter_at(canvas_rect.intersect(panel_rect));

        painter.rect_filled(canvas_rect, 0.0, Color32::WHITE);
        painter.rect_stroke(canvas_rect, 0.0,
            egui::Stroke::new(1.0, Color32::from_rgb(0xD0, 0xD0, 0xD0)));

        self.camera.viewport = [canvas_w, canvas_h];
        self.camera.zoom = zoom;
        self.camera.offset = [page_w * 0.5, page_h * 0.5];

        // Use Ui::allocate_rect which respects panel bounds
        let sense = Sense::click_and_drag();
        let response = ui.allocate_rect(canvas_rect, sense);

        if let Some(pos) = response.hover_pos() {
            let world = self.camera.screen_to_world(pos);
            self.interaction.cursor_screen = Some(pos);
            self.interaction.cursor_world = Some(world);
        }

        self.board.update(ui, &response, &self.camera);

        let idx = self.multi_page.current_page;
        if let Some(page) = self.doc.pages.get_mut(idx) {
            page.elements.clone_from(&self.board.elements);
        }

        crate::render::render_canvas(&painter, &self.doc, &self.camera, &self.interaction, idx);
        self.annotation.paint(&painter);
        self.board.render_overlay(&painter, &self.camera);
    }
}

impl eframe::App for EditApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // ── 强制深色主题：覆盖 Windows 系统主题偏好，防止浅色模式下文字不可见 ──
        ctx.set_theme(egui::ThemePreference::Dark);

        // ── 更新父窗口句柄（用于文件对话框绑定，防止遮挡）──
        if let Ok(handle) = frame.window_handle() {
            self.parent_window = Some(ParentWindow(handle.as_raw()));
        }

        // ── Top toolbar ────────────────────────────────────────────
        let parent = self.parent_window;
        egui::TopBottomPanel::top("editor_toolbar")
            .min_height(36.0)
            .frame(egui::Frame::none()
                .fill(TOOLBAR_BG)
                .inner_margin(egui::Margin::symmetric(12.0, 4.0)))
            .show(ctx, |ui| {
                let v = ui.visuals_mut();
                v.widgets.noninteractive.fg_stroke.color = Color32::from_rgb(0xE0, 0xE0, 0xE0);
                v.widgets.inactive.fg_stroke.color = Color32::from_rgb(0xE0, 0xE0, 0xE0);
                ui.horizontal(|ui| {
                    ui.menu_button(" 文件", |ui| {
                        if ui.button("打开...").clicked() {
                            let mut dialog = rfd::FileDialog::new()
                                .add_filter("Drafftink", &["drft", "enbx"]);
                            if let Some(ref p) = parent {
                                dialog = dialog.set_parent(p);
                            }
                            if let Some(picked) = dialog.pick_file() {
                                self.open_file(&picked);
                            }
                            ui.close_menu();
                        }
                        if ui.button("保存").clicked() {
                            let mut dialog = rfd::FileDialog::new()
                                .add_filter("Drafftink", &["drft"]);
                            if let Some(ref p) = parent {
                                dialog = dialog.set_parent(p);
                            }
                            if let Some(picked) = dialog.save_file() {
                                self.save_drft(&picked);
                            }
                            ui.close_menu();
                        }
                    });
                    ui.separator();
                    if label_btn(ui, "T 文本", TOOLBAR_BG).clicked() {
                        self.text_insert_requested = Some(());
                    }
                    if label_btn(ui, " 形状", TOOLBAR_BG).clicked() {
                        self.interaction.mode = ToolMode::DrawShape(ShapeType::Rectangle);
                    }
                    if label_btn(ui, "🎬 多媒体", TOOLBAR_BG).clicked() {
                        self.media_pick_requested = true;
                    }
                    if label_btn(ui, "🖼 图片", TOOLBAR_BG).clicked() {
                        self.image_pick_requested = true;
                    }
                    if label_btn(ui, "🎵 音频", TOOLBAR_BG).clicked() {
                        self.audio_pick_requested = true;
                    }
                    if label_btn(ui, "💾 保存", TOOLBAR_BG).clicked() {
                        self.enbx_save_requested = true;
                    }
                    // 「📐 教具」下拉：选择虚拟教具（由宿主激活画布覆盖层）。
                    ui.menu_button("📐 教具", |ui| {
                        for kind in [
                            TeachingToolKind::Compass,
                            TeachingToolKind::SetSquare30,
                            TeachingToolKind::SetSquare45,
                            TeachingToolKind::Protractor,
                            TeachingToolKind::Ruler,
                            TeachingToolKind::NumberLine,
                            TeachingToolKind::FunctionPlot,
                        ] {
                            if ui.button(kind.label()).clicked() {
                                self.tool_requested = Some(kind);
                                ui.close_menu();
                            }
                        }
                        ui.menu_button("📐 正多边形 ▶", |ui| {
                            for n in 3..=12_u8 {
                                if ui.button(TeachingToolKind::Polygon(n).label()).clicked() {
                                    self.tool_requested = Some(TeachingToolKind::Polygon(n));
                                    ui.close_menu();
                                }
                            }
                        });
                    });
                    // 「🔷 形状」选择器：下拉选形状种类 → 「➕ 插入」直接插入画布中心。
                    egui::ComboBox::from_id_salt("shape_selector")
                        .selected_text(format!("🔷 {}", shape_kind_label(self.selected_shape)))
                        .show_ui(ui, |combo| {
                            for k in ShapeKind::ALL {
                                combo
                                    .selectable_value(&mut self.selected_shape, k, shape_kind_label(k))
                                    .on_hover_text(shape_kind_label(k));
                            }
                        });
                    if ui.button("➕ 插入").clicked() {
                        self.shape_insert_requested = Some(self.selected_shape);
                    }
                    if label_btn(ui, " 表格", TOOLBAR_BG).clicked() {}
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_sized([72.0, 24.0],
                            egui::Button::new(
                                egui::RichText::new(" 保存").color(Color32::WHITE).strong()
                            )
                            .fill(Color32::from_rgb(0x2D, 0x6C, 0xDF))
                            .rounding(egui::Rounding::same(6.0))
                        ).clicked() {
                            self.save_requested = true;
                        }
                        if ui.add_sized([88.0, 24.0],
                            egui::Button::new(
                                egui::RichText::new(" 授课").color(Color32::WHITE).strong()
                            )
                            .fill(Color32::from_rgb(0x07, 0xC1, 0x60))
                            .rounding(egui::Rounding::same(6.0))
                        ).clicked() {
                            self.teach_requested = true;
                        }
                    });
                });
            });

        // ── Left page list ─────────────────────────────────────────
        egui::SidePanel::left("page_list")
            .default_width(80.0)
            .resizable(false)
            .frame(egui::Frame::none()
                .fill(SIDEBAR_BG)
                .stroke(egui::Stroke::new(1.0, Color32::from_rgb(0xC0, 0xC0, 0xC0))))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    if ui.button("+\n新建").clicked() {
                        self.doc.pages.push(Default::default());
                        self.multi_page.add_page();
                        let snap = Snapshot::from_doc(&self.doc, self.multi_page.current_page);
                        self.board.load_snapshot(&snap);
                        self.annotation.clear_screen();
                    }
                    ui.add_space(12.0);
                    for i in 0..self.doc.pages.len().max(1) {
                        let active = i == self.multi_page.current_page;
                        // Allocate a clickable rect, then paint it manually
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(60.0, 32.0),
                            egui::Sense::click(),
                        );
                        // Draw fill and border
                        let bg = if active {
                            Color32::from_rgb(0xF0, 0xFF, 0xF0)
                        } else {
                            Color32::WHITE
                        };
                        let border = if active {
                            (3.0, PAGE_ACTIVE)
                        } else {
                            (1.0, PAGE_INACTIVE)
                        };
                        ui.painter().rect_filled(
                            rect,
                            4.0,
                            bg,
                        );
                        ui.painter().rect_stroke(
                            rect,
                            4.0,
                            egui::Stroke::new(border.0, border.1),
                        );
                        // Draw page number centered
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            format!("{}", i + 1),
                            egui::FontId {
                                size: 14.0,
                                family: egui::FontFamily::Proportional,
                            },
                            Color32::BLACK,
                        );
                        if response.clicked() && i != self.multi_page.current_page {
                            // Flush current page edits to doc before switching
                            if let Some(page) = self.doc.pages.get_mut(self.multi_page.current_page) {
                                page.elements.clone_from(&self.board.elements);
                            }
                            self.multi_page.save_annotations(&self.annotation);
                            self.multi_page.current_page = i;
                            self.annotation.clear_screen();
                            self.multi_page.load_annotations(&mut self.annotation);
                            // Load new page into edit board
                            let snap = Snapshot::from_doc(&self.doc, i);
                            self.board.load_snapshot(&snap);
                        }
                        ui.add_space(8.0);
                    }
                });
            });

        // ── Right inspector ────────────────────────────────────────
        egui::SidePanel::right("inspector")
            .min_width(250.0)
            .frame(egui::Frame::none()
                .fill(SIDEBAR_BG)
                .stroke(egui::Stroke::new(1.0, Color32::from_rgb(0xC0, 0xC0, 0xC0)))
                .inner_margin(egui::Margin::same(10.0)))
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.label(egui::RichText::new("布局与背景").size(14.0).strong());
                ui.add_space(8.0);
                ui.group(|ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(egui::RichText::new("页面设置").size(12.0).strong());
                });
                ui.add_space(8.0);
                ui.group(|ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(egui::RichText::new("背景").size(12.0).strong());
                });
                ui.add_space(8.0);
                if ui.button("应用主题").clicked() {}
            });

        // ── Central canvas ─────────────────────────────────────────
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(CANVAS_BG))
            .show(ctx, |ui| {
                self.render_canvas_area(ui);
            });
    }
}

fn label_btn(ui: &mut Ui, label: &str, bg: Color32) -> egui::Response {
    ui.add(
        egui::Button::new(
            egui::RichText::new(label).size(12.0).color(Color32::from_rgb(0xE0, 0xE0, 0xE0))
        )
        .fill(bg)
        .frame(false)
        .min_size(egui::vec2(0.0, 24.0))
    )
}
