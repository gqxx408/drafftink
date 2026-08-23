//! Drafftink Display — fullscreen presentation with unified bottom toolbar.

use drafftink_core::board::{DisplayBoard, Snapshot};
use drafftink_core::document;
use drafftink_core::model::CoursewareDoc;
use drafftink_core::plugin::PluginManager;
use drafftink_core::plugin::api::DummyContext;
use drafftink_core::Camera;
use egui::Color32;
use raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, WindowHandle};
use std::sync::{Arc, Mutex};
use drafftink_core::integration::SharedAppContext;

use crate::annotation::{AnnotationSystem, ToolbarAction, ToolType};
use crate::interaction::InteractionState;
use crate::multi_page::MultiPageState;
use crate::physics::PhysicsEditor;
use crate::workshop::Workshop;
use drafftink_cosmos::CosmosViewer;
use drafftink_mindmap::MindMapViewer;
use drafftink_functions::FunctionViewer;
use drafftink_geometry::{GeometryViewer, SolarSystemViewer};

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

// Phase 3 悬浮工具面板配色（与底部工具栏视觉一致）
const FLOAT_ACTIVE: Color32 = Color32::from_rgb(60, 120, 220);
const FLOAT_INACTIVE: Color32 = Color32::from_rgb(220, 220, 220);
const FLOAT_TEXT: Color32 = Color32::from_rgb(30, 30, 30);

pub struct DisplayApp {
    pub doc: CoursewareDoc,
    pub camera: Camera,
    pub interaction: InteractionState,
    pub multi_page: MultiPageState,
    pub board: DisplayBoard,
    pub doc_path: Option<String>,
    pub annotations: AnnotationSystem,
    /// 共享插件管理器：由宿主注入（备授一体，复用同一实例、避免 cdylib 双加载），
    /// 或独立运行（display.exe）时由 `new` 自建。
    pub plugin_manager: Arc<Mutex<PluginManager>>,
    /// Set true after loading a new doc — triggers request_discard() to avoid flash.
    just_loaded_doc: bool,
    frame_count: u64,
    /// Physics tool editor (opens as separate fullscreen mode)
    physics_editor: Option<PhysicsEditor>,
    /// Workshop (multi-subject resource system)
    workshop: Option<Workshop>,
    /// Cosmos viewer (3D solar system visualization)
    cosmos_viewer: Option<CosmosViewer>,
    /// Mind map viewer
    mindmap: Option<MindMapViewer>,
    /// Function plotter viewer
    functions_viewer: Option<FunctionViewer>,
    /// Geometry viewer (dynamic geometry + 3D primitives)
    geometry_viewer: Option<GeometryViewer>,
    /// Solar system viewer (EasiNote geography slides)
    solar_system_viewer: Option<SolarSystemViewer>,
    /// 父窗口句柄（用于文件对话框 set_parent，防止遮挡）
    parent_window: Option<ParentWindow>,
    /// 备授一体共享上下文（由宿主注入）。仅用于上层状态传递，不进入核心逻辑。
    pub shared: Option<Arc<Mutex<SharedAppContext>>>,
    /// 授课端 Esc 置位，由宿主读取后切回备课模式（替代原 `std::process::exit`）。
    pub exit_to_prepare: bool,
    /// Phase 3：左上悬浮工具面板可见性（仅 UI 层，不进入核心逻辑）。
    pub show_tools: bool,
    /// Phase 3：右上「下一页预览」小窗可见性。
    pub show_preview: bool,
    /// Phase 3：板书导出结果提示（仅 UI 展示，最近一次导出路径或错误）。
    board_export_msg: Option<String>,
}

impl DisplayApp {
    pub fn new(
        document: CoursewareDoc,
        doc_path: Option<String>,
        shared_pm: Option<Arc<Mutex<PluginManager>>>,
    ) -> Self {
        let multi = MultiPageState::from_doc(&document);
        let snap = Snapshot::from_doc(&document, 0);
        let mut board = DisplayBoard::default();
        board.load_snapshot(&snap);

        let doc_hash = if let Some(ref p) = doc_path {
            let file_data = std::fs::read(p).unwrap_or_default();
            crc32fast::hash(&file_data)
        } else {
            0
        };

        let cache_dir = std::env::temp_dir().join("drafftink").join("cache");
        let annotations = AnnotationSystem::new(doc_hash, cache_dir);

        // ── Plugin system ──
        // Search order: 1) plugins/ next to exe  2) cwd plugins/  3) %APPDATA%
        let mut plugin_dir = get_plugins_dir(); // fallback
        // Try exe-relative path first (works for both cargo run and cargo build)
        if let Ok(exe) = std::env::current_exe() {
            // Walk up from target/release/display.exe to project root, then plugins/
            let mut candidate = exe.clone();
            for _ in 0..4 {
                candidate.pop(); // remove file, then go up
                let p = candidate.join("plugins");
                if p.exists() {
                    plugin_dir = p;
                    break;
                }
            }
        }
        // Also try cwd-relative
        if !plugin_dir.exists() {
            let cwd = std::path::PathBuf::from("./plugins");
            if cwd.exists() {
                plugin_dir = cwd;
            }
        }
        // 共享插件管理器优先：由宿主注入则直接复用（不在每次进入授课模式时重复加载）；
        // 独立运行（display.exe）时自建并加载一次。
        let plugin_manager = shared_pm.unwrap_or_else(|| {
            log::info!("[display] Plugin dir: {:?}", plugin_dir);
            let mut pm = PluginManager::new(plugin_dir);
            // Discover and load plugins (best-effort; failures are logged, not fatal)
            let discovered = pm.discover();
            log::info!("[display] Found {} plugin(s)", discovered.len());
            let dummy_ctx = DummyContext;
            // Safety: plugins are trusted cdylibs compiled in the same workspace
            unsafe {
                for path in discovered {
                    if let Err(e) = pm.load(&path, &dummy_ctx) {
                        log::error!("[display] Plugin load failed: {:?} — {}", path, e);
                    }
                }
            }
            Arc::new(Mutex::new(pm))
        });

        let viewport = [1920.0, 1080.0];
        let mut cam = Camera {
            offset: [0.0, 0.0],
            zoom: 1.0,
            viewport,
        };
        Self::fit_camera_to_doc(&mut cam, &document, viewport);

        Self {
            doc: document,
            camera: cam,
            interaction: InteractionState::new(),
            multi_page: multi,
            board,
            doc_path,
            annotations,
            plugin_manager,
            just_loaded_doc: false,
            frame_count: 0,
            physics_editor: None,
            workshop: None,
            cosmos_viewer: None,
            mindmap: None,
            functions_viewer: None,
            geometry_viewer: None,
            solar_system_viewer: None,
            parent_window: None,
            shared: None,
            exit_to_prepare: false,
            show_tools: true,
            show_preview: true,
            board_export_msg: None,
        }
    }

    /// 注入备授一体共享上下文（宿主在构造后调用）。
    pub fn set_shared(&mut self, ctx: Arc<Mutex<SharedAppContext>>) {
        self.shared = Some(ctx);
    }

    /// Fit the camera so the page fills the viewport with a small margin.
    fn fit_camera_to_doc(cam: &mut Camera, doc: &CoursewareDoc, viewport: [f32; 2]) {
        cam.viewport = viewport;
        let pw = doc.page_size[0].max(1.0);
        let ph = doc.page_size[1].max(1.0);
        let vw = viewport[0].max(1.0);
        let vh = viewport[1].max(1.0);
        // Fit the whole page inside the viewport with a small margin, using the
        // SAME formula as drafftink-edit (avail - 2*margin)/page. This keeps the
        // display-mode layout pixel-consistent with edit mode so shapes do not
        // appear "slightly off" when the same .enbx is opened in both.
        let margin = 24.0_f32;
        let scale_x = (vw - margin * 2.0) / pw;
        let scale_y = (vh - margin * 2.0) / ph;
        cam.zoom = scale_x.min(scale_y).max(0.05);
        cam.offset = [pw * 0.5, ph * 0.5];
        log::info!(
            "[camera] fit_to_page: page={}x{} viewport={}x{} zoom={:.3} offset={:?}",
            pw, ph, vw, vh, cam.zoom, cam.offset
        );
    }

    fn open_file_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Drafftink 课件", &["drft", "enbx", "enbxz"])
            .set_title("打开课件");
        // 绑定父窗口句柄，确保对话框始终显示在主窗口之上
        if let Some(ref parent) = self.parent_window {
            dialog = dialog.set_parent(parent);
        }
        if let Some(picked) = dialog.pick_file() {
            // Determine format by extension
            let ext = picked
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            let result = if ext == "enbx" || ext == "enbxz" {
                // Use built-in format_enbx loader directly (avoids plugin DLL issues)
                match std::fs::read(&picked) {
                    Ok(data) => {
                        let ctx = DummyContext;
                        format_enbx::loader::load_enbx(&data, &ctx)
                    }
                    Err(e) => Err(format!("read file failed: {}", e)),
                }
            } else {
                // Use native loader for .drft files
                document::load_document(&picked).map_err(|e| e.to_string())
            };

            match result {
                Ok(doc) => {
                    log::info!("[app] Loaded doc: {} pages, page_size={:?}",
                        doc.pages.len(), doc.page_size);
                    self.doc = doc;
                    self.doc_path = Some(picked.to_string_lossy().into_owned());
                    self.multi_page = MultiPageState::from_doc(&self.doc);
                    let snap = Snapshot::from_doc(&self.doc, 0);
                    self.board.load_snapshot(&snap);
                    self.annotations.clear();
                    self.analyze_smart_alpha();
                    // Fit camera to the newly loaded document
                    let vp = self.camera.viewport;
                    Self::fit_camera_to_doc(&mut self.camera, &self.doc, vp);
                    self.just_loaded_doc = true;
                    log::info!("[app] Flagged request_discard for next frame");
                }
                Err(e) => {
                    log::error!("[app] Open failed: {}", e);
                    log::error!("Open failed: {}", e);
                }
            }
        }
    }

    /// Load an .enbx file using the format_enbx plugin importer.
    #[allow(dead_code)]
    fn load_via_plugin(&self, path: &std::path::Path) -> Result<CoursewareDoc, String> {
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        log::info!("[app] Loading .enbx: {} bytes", data.len());

        // Find the format_enbx importer from loaded plugins
        let guard = self.plugin_manager.lock().unwrap();
        let importers = guard.all_importers();
        log::info!("[app] Available importers: {}", importers.len());

        for (name, importer) in &importers {
            log::info!("[app] Trying importer: {}", name);
            if importer.can_import(&data) {
                let ctx = DummyContext;
                return importer.import(&data, &ctx);
            }
        }
        Err("No plugin supports this .enbx file".into())
    }

    fn apply_action(&mut self, action: ToolbarAction) {
        match action {
            ToolbarAction::None => {}
            ToolbarAction::NewPage => {
                self.doc.pages.push(Default::default());
                self.multi_page.add_page();
                let snap = Snapshot::from_doc(&self.doc, self.multi_page.current_page);
                self.board.load_snapshot(&snap);
                self.annotations.clear();
                self.analyze_smart_alpha();
            }
            ToolbarAction::PrevPage => {
                if self.multi_page.current_page > 0 {
                    self.multi_page.current_page -= 1;
                    let snap =
                        Snapshot::from_doc(&self.doc, self.multi_page.current_page);
                    self.board.load_snapshot(&snap);
                    self.analyze_smart_alpha();
                }
            }
            ToolbarAction::NextPage => {
                if self.multi_page.current_page + 1 < self.doc.pages.len() {
                    self.multi_page.current_page += 1;
                    let snap =
                        Snapshot::from_doc(&self.doc, self.multi_page.current_page);
                    self.board.load_snapshot(&snap);
                    self.analyze_smart_alpha();
                }
            }
            ToolbarAction::Exit => {
                self.annotations.shutdown();
                std::process::exit(0);
            }
            ToolbarAction::ToggleMore => {
                self.annotations.toolbar.more_menu_open =
                    !self.annotations.toolbar.more_menu_open;
            }
        }
    }

    fn analyze_smart_alpha(&mut self) {
        let idx = self.multi_page.current_page;
        let bg = self.doc.background_color;
        let bg3: [u8; 3] = [bg[0], bg[1], bg[2]];
        let element_count = if idx < self.doc.pages.len() {
            self.doc.pages[idx].elements.len()
        } else {
            self.doc.elements.len()
        };
        let w = self.doc.page_size[0];
        let h = self.doc.page_size[1];
        self.annotations
            .analyze_current_page(idx, &bg3, element_count, w, h);
    }

    /// 构造一个把指定页适配到 `rect`(绝对屏幕矩形)的相机，用于预览小窗渲染。
    /// 仅做坐标计算，不修改任何渲染核心逻辑：通过把视口中心编码为窗口中心，
    /// 让 `world_to_screen` 把页面内容映射到窗口绝对区域（由 painter 的 clip 限制）。
    fn preview_camera(&self, rect: egui::Rect, _page_index: usize) -> Camera {
        let pw = self.doc.page_size[0].max(1.0);
        let ph = self.doc.page_size[1].max(1.0);
        let vw = rect.width().max(1.0);
        let vh = rect.height().max(1.0);
        let zoom = (vw / pw).min(vh / ph) * 0.92;
        let center = rect.center();
        // viewport/2 即 world_to_screen 的平移量；令其等于窗口中心，
        // 则 focal(offset = 页面中心) 恰好落在窗口中心。
        Camera {
            zoom,
            offset: [pw * 0.5, ph * 0.5],
            viewport: [2.0 * center.x, 2.0 * center.y],
        }
    }

    /// 弹出保存对话框，把当前页板书批注导出为独立的 `.drfp` 文件。
    /// 仅写入新文件，绝不修改课件本身（学生作答快照合规红线）。
    fn export_board_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .set_file_name("板书批注.drfp")
            .add_filter("板书批注 (*.drfp)", &["drfp"])
            .set_title("导出板书批注");
        if let Some(ref parent) = self.parent_window {
            dialog = dialog.set_parent(parent);
        }
        if let Some(path) = dialog.save_file() {
            match self.annotations.export_to(&path) {
                Ok(()) => {
                    self.board_export_msg = Some(format!("已导出板书: {}", path.display()));
                    log::info!("[display] 板书已导出: {}", path.display());
                }
                Err(e) => {
                    self.board_export_msg = Some(format!("导出失败: {e}"));
                    log::error!("[display] 板书导出失败: {e}");
                }
            }
        }
    }
}

/// Platform-aware plugins directory.
fn get_plugins_dir() -> std::path::PathBuf {
    #[cfg(windows)]
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    #[cfg(not(windows))]
    let base = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(base).join("drafftink").join("plugins")
}

impl eframe::App for DisplayApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // ── 强制深色主题：覆盖 Windows 系统主题偏好，防止浅色模式下文字不可见 ──
        ctx.set_theme(egui::ThemePreference::Dark);

        // ── 更新父窗口句柄（用于文件对话框绑定，防止遮挡）──
        if let Ok(handle) = frame.window_handle() {
            self.parent_window = Some(ParentWindow(handle.as_raw()));
        }

        // ── Discard first frame after loading a new doc to eliminate layout flash ──
        if self.just_loaded_doc {
            ctx.request_discard("课件首次加载，消除首帧抖动");
            self.just_loaded_doc = false;
            log::info!("[app] request_discard called to eliminate flash");
        }

        // ── Update camera viewport to match actual screen size ──
        let screen_rect = ctx.screen_rect();
        let viewport = [screen_rect.width(), screen_rect.height()];
        self.camera.viewport = viewport;

        // Re-fit the camera every frame so `zoom` always tracks the real viewport.
        // `new()` initially fits against a placeholder [1920,1080] viewport and the
        // only other fit happens on file-open; without this, a document injected at
        // construction (host integration) or any window resize leaves the zoom frozen
        // against the wrong viewport, shifting/scaling every shape (the reported
        // "坐标有点偏"). Display mode has no user pan/zoom, so re-fitting is safe.
        // Pass `viewport` (a local copy) so we don't borrow `self.camera` both
        // mutably and immutably in the same call (E0503).
        Self::fit_camera_to_doc(&mut self.camera, &self.doc, viewport);

        // ── Debug: log doc state every 60 frames ──
        self.frame_count += 1;
        if self.frame_count % 60 == 0 {
            log::info!(
                "[render] frame={} pages={} path={:?} page_size={:?} viewport={:?} offset={:?} zoom={}",
                self.frame_count,
                self.doc.pages.len(),
                self.doc_path.as_deref().unwrap_or("none"),
                self.doc.page_size,
                self.camera.viewport,
                self.camera.offset,
                self.camera.zoom,
            );
        }

        // Keyboard shortcuts for save & exit
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.annotations.shutdown();
            // 不再退出进程：通知宿主切回备课模式（上层整合钩子）。
            self.exit_to_prepare = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::S)) {
            if let Some(ref path) = self.doc_path {
                match self.annotations.save_patch(&std::path::PathBuf::from(path)) {
                    Ok(()) => {
                        self.annotations.cache.cleanup();
                    }
                    Err(e) => log::error!("Save failed: {}", e),
                }
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight))
            || ctx.input(|i| i.key_pressed(egui::Key::PageDown))
        {
            self.apply_action(ToolbarAction::NextPage);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft))
            || ctx.input(|i| i.key_pressed(egui::Key::PageUp))
        {
            self.apply_action(ToolbarAction::PrevPage);
        }

        // ── Physics editor: if open, takes over the whole screen ──
        if self.physics_editor.is_some() {
            if let Some(ref mut editor) = self.physics_editor {
                editor.ui(ctx);
            }
            // Close button (top-left corner) to go back to courseware mode
            let _close_rect = egui::Rect::from_min_size(
                egui::pos2(12.0, 12.0),
                egui::vec2(100.0, 36.0),
            );
            let _close_response = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Tooltip,
                egui::Id::new("physics_close_btn"),
            ));
            // Use a button area instead
            egui::Area::new(egui::Id::new("physics_close"))
                .fixed_pos(egui::pos2(12.0, 12.0))
                .show(ctx, |ui| {
                    if ui.button("← 返回课件").clicked() {
                        self.physics_editor = None;
                    }
                });
            return;
        }

        // ── Workshop: if open, takes over the whole screen ──
        if self.workshop.is_some() {
            if let Some(ref mut ws) = self.workshop {
                ws.ui(ctx);
            }
            egui::Area::new(egui::Id::new("workshop_close"))
                .fixed_pos(egui::pos2(12.0, 12.0))
                .show(ctx, |ui| {
                    if ui.button("← 返回课件").clicked() {
                        self.workshop = None;
                    }
                });
            return;
        }

        // ── Cosmos viewer: if open, takes over the whole screen ──
        if self.cosmos_viewer.is_some() {
            if let Some(ref mut viewer) = self.cosmos_viewer {
                viewer.ui(ctx);
            }
            egui::Area::new(egui::Id::new("cosmos_close"))
                .fixed_pos(egui::pos2(12.0, 12.0))
                .show(ctx, |ui| {
                    if ui.button("← 返回课件").clicked() {
                        self.cosmos_viewer = None;
                    }
                });
            return;
        }

        // ── Mind map: if open, takes over the whole screen ──
        if self.mindmap.is_some() {
            if let Some(ref mut viewer) = self.mindmap {
                viewer.ui(ctx);
            }
            egui::Area::new(egui::Id::new("mindmap_close"))
                .anchor(egui::Align2::RIGHT_TOP, [-12.0, 12.0])
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    if ui.button("← 返回课件").clicked() {
                        self.mindmap = None;
                    }
                });
            return;
        }

        // ── Function plotter: if open, takes over the whole screen ──
        if self.functions_viewer.is_some() {
            if let Some(ref mut viewer) = self.functions_viewer {
                viewer.ui(ctx);
            }
            egui::Area::new(egui::Id::new("functions_close"))
                .anchor(egui::Align2::RIGHT_TOP, [-12.0, 12.0])
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    if ui.button("← 返回课件").clicked() {
                        self.functions_viewer = None;
                    }
                });
            return;
        }

        // ── Solar system viewer: if open, takes over the whole screen ──
        if self.solar_system_viewer.is_some() {
            if let Some(ref mut viewer) = self.solar_system_viewer {
                viewer.ui(ctx);
                if viewer.should_close {
                    self.solar_system_viewer = None;
                }
            }
            return;
        }

        // ── Geometry viewer: if open, takes over the whole screen ──
        if self.geometry_viewer.is_some() {
            if let Some(ref mut viewer) = self.geometry_viewer {
                viewer.ui(ctx);
            }
            egui::Area::new(egui::Id::new("geometry_close"))
                .anchor(egui::Align2::RIGHT_TOP, [-12.0, 12.0])
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    if ui.button("← 返回课件").clicked() {
                        self.geometry_viewer = None;
                    }
                });
            return;
        }
        let bg_painter = ctx.layer_painter(egui::LayerId::background());
        bg_painter.rect_filled(screen_rect, 0.0, Color32::from_rgb(0x1A, 0x3C, 0x1A));

        if self.doc_path.is_some() {
            if self.doc.pages.is_empty() && self.doc.elements.is_empty() {
                // Document loaded but no pages parsed — show diagnostic hint
                egui::Area::new(egui::Id::new("empty_doc_hint"))
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label("课件导入成功，但未解析到页面（请检查导入日志）");
                    });
            } else {
                crate::render::render_canvas(
                    &bg_painter,
                    &self.doc,
                    &self.camera,
                    &self.interaction,
                    self.multi_page.current_page,
                );
            }
        }

        // Annotation system (input + toolbar + render + cache)
        let page_current = self.multi_page.current_page;
        let page_total = self.doc.pages.len().max(1);
        let action = self.annotations.update(
            ctx,
            screen_rect,
            page_current,
            page_total,
        );

        self.apply_action(action);

        // More menu popup — rendered separately to avoid consuming annotation input
        if self.annotations.toolbar.more_menu_open {
            egui::Area::new(egui::Id::new("more_popup"))
                .anchor(egui::Align2::RIGHT_BOTTOM, [-12.0, -40.0])
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    ui.set_max_width(180.0);
                    egui::Frame::none()
                        .fill(Color32::from_rgba_premultiplied(245, 245, 245, 250))
                        .rounding(egui::Rounding::same(8.0))
                        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(180, 180, 180)))
                        .inner_margin(egui::Margin::same(8.0))
                        .show(ui, |ui| {
                            // 白底黑字：覆盖深色主题
                            ui.visuals_mut().widgets.noninteractive.fg_stroke.color = Color32::from_rgb(30, 30, 30);
                            ui.visuals_mut().widgets.inactive.fg_stroke.color = Color32::from_rgb(30, 30, 30);
                            ui.visuals_mut().widgets.hovered.fg_stroke.color = Color32::from_rgb(30, 30, 30);
                            ui.visuals_mut().widgets.hovered.bg_fill = Color32::from_rgb(220, 220, 220);
                            ui.visuals_mut().widgets.active.fg_stroke.color = Color32::from_rgb(30, 30, 30);
                            if ui.button("打开课件").clicked() {
                                self.annotations.toolbar.more_menu_open = false;
                                self.open_file_dialog();
                            }
                            ui.separator();
                            if ui.button("🎓 学科工坊").clicked() {
                                self.annotations.toolbar.more_menu_open = false;
                                self.workshop = Some(Workshop::new());
                            }
                            ui.separator();
                            if ui.button("⚡ 物理工具").clicked() {
                                self.annotations.toolbar.more_menu_open = false;
                                self.physics_editor = Some(PhysicsEditor::new());
                            }
                            ui.separator();
                            if ui.button("🌌 太阳系 3D").clicked() {
                                self.annotations.toolbar.more_menu_open = false;
                                self.cosmos_viewer = Some(CosmosViewer::new());
                            }
                            ui.separator();
                            if ui.button("🧠 思维导图").clicked() {
                                self.annotations.toolbar.more_menu_open = false;
                                self.mindmap = Some(MindMapViewer::new());
                            }
                            ui.separator();
                            if ui.button("📊 函数绘图").clicked() {
                                self.annotations.toolbar.more_menu_open = false;
                                self.functions_viewer = Some(FunctionViewer::new());
                            }
                            ui.separator();
                            if ui.button("📐 动态几何").clicked() {
                                self.annotations.toolbar.more_menu_open = false;
                                self.geometry_viewer = Some(GeometryViewer::new());
                            }
                            ui.separator();
                            if ui.button("🌍 地理星球").clicked() {
                                self.annotations.toolbar.more_menu_open = false;
                                self.solar_system_viewer = Some(SolarSystemViewer::new());
                            }
                            ui.separator();
                            if ui.button("清除批注").clicked() {
                                self.annotations.toolbar.more_menu_open = false;
                                self.annotations.clear();
                            }
                            ui.separator();
                            if ui.button("关于").clicked() {
                                self.annotations.toolbar.more_menu_open = false;
                            }
                        });
                });
        }

        // ── Phase 3: 悬浮工具面板（左上，常驻）+ 下一页预览小窗（右上）──
        if ctx.input(|i| i.key_pressed(egui::Key::W)) {
            self.show_preview = !self.show_preview;
        }

        if self.show_tools {
            egui::Window::new("授课工具")
                .anchor(egui::Align2::LEFT_TOP, [12.0, 12.0])
                .resizable(false)
                .collapsible(true)
                .show(ctx, |ui| {
                    ui.visuals_mut().widgets.noninteractive.fg_stroke.color = FLOAT_TEXT;
                    ui.visuals_mut().widgets.inactive.fg_stroke.color = FLOAT_TEXT;
                    ui.visuals_mut().widgets.hovered.fg_stroke.color = FLOAT_TEXT;
                    ui.visuals_mut().widgets.active.fg_stroke.color = FLOAT_TEXT;
                    ui.horizontal(|ui| {
                        if ui
                            .button(if self.show_preview { "👁 预览:开" } else { "👁 预览:关" })
                            .clicked()
                        {
                            self.show_preview = !self.show_preview;
                        }
                        if ui.button("🗑 清空白板").clicked() {
                            self.annotations.clear();
                        }
                    });
                    ui.horizontal(|ui| {
                        let pen_active = matches!(self.annotations.tool, ToolType::Pen);
                        let hl_active = matches!(self.annotations.tool, ToolType::Highlighter);
                        let er_active = matches!(self.annotations.tool, ToolType::Eraser);
                        if ui
                            .add(
                                egui::Button::new("✏ 笔")
                                    .fill(if pen_active { FLOAT_ACTIVE } else { FLOAT_INACTIVE }),
                            )
                            .clicked()
                        {
                            self.annotations.tool = ToolType::Pen;
                            self.annotations.color = [0, 0, 0, 255];
                            self.annotations.thickness = 2.5;
                        }
                        if ui
                            .add(
                                egui::Button::new("🖍 荧光")
                                    .fill(if hl_active { FLOAT_ACTIVE } else { FLOAT_INACTIVE }),
                            )
                            .clicked()
                        {
                            self.annotations.tool = ToolType::Highlighter;
                            self.annotations.color = [255, 230, 0, 140];
                            self.annotations.thickness = 12.0;
                        }
                        if ui
                            .add(
                                egui::Button::new("✕ 橡皮")
                                    .fill(if er_active { FLOAT_ACTIVE } else { FLOAT_INACTIVE }),
                            )
                            .clicked()
                        {
                            self.annotations.tool = ToolType::Eraser;
                            self.annotations.thickness = 12.0;
                        }
                    });
                    if ui.button("📤 板书导出").clicked() {
                        self.export_board_dialog();
                    }
                    if let Some(ref msg) = self.board_export_msg {
                        ui.label(
                            egui::RichText::new(msg)
                                .size(11.0)
                                .color(Color32::from_rgb(40, 120, 40)),
                        );
                    }
                    ui.label(format!("第 {}/{} 页", page_current + 1, page_total));
                });
        }

        if self.show_preview {
            let next = self.multi_page.current_page + 1;
            egui::Window::new("下一页预览")
                .anchor(egui::Align2::RIGHT_TOP, [-12.0, 12.0])
                .resizable(true)
                .default_width(240.0)
                .show(ctx, |ui| {
                    ui.visuals_mut().widgets.noninteractive.fg_stroke.color = FLOAT_TEXT;
                    let rect = ui.max_rect();
                    if next < self.doc.pages.len() {
                        let cam = self.preview_camera(rect, next);
                        let empty: InteractionState = Default::default();
                        crate::render::render_canvas(ui.painter(), &self.doc, &cam, &empty, next);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("已是最后一页");
                        });
                    }
                });
        }
    }
}
