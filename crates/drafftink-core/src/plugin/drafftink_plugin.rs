//! Enhanced plugin interface — `DrafftinkPlugin` trait with richer
//! host interaction capabilities.
//!
//! Compared to the existing `Plugin` trait (focused on file importers),
//! `DrafftinkPlugin` allows plugins to:
//!   - Register toolbar buttons
//!   - Add UI panels (sidebars, overlays, full-screen views)
//!   - Access and modify the document model
//!   - Register custom element types (geometry, physics, etc.)
//!
//! Plugins are compiled as `cdylib` and loaded at runtime via `libloading`.
//! The `#[export_plugin]` proc-macro from `drafftink-plugin-macros`
//! generates the FFI entry point.

use crate::model::CoursewareDoc;
use egui::Context;
use std::sync::Arc;

// ── Toolbar Action ────────────────────────────────────────────────

/// A button or action that a plugin wants to add to the host toolbar.
#[derive(Clone)]
pub struct ToolbarAction {
    /// Display label (supports emoji).
    pub label: String,
    /// Tooltip shown on hover.
    pub tooltip: String,
    /// Unique identifier for deduplication.
    pub id: String,
    /// Callback invoked when the button is clicked.
    /// Receives a reference to the egui context for spawning windows/popups.
    pub on_click: Arc<dyn Fn(&Context) + Send + Sync>,
}

// ── UI Panel ──────────────────────────────────────────────────────

/// Describes a panel that a plugin can add to the host UI.
#[derive(Clone)]
pub enum UiPanel {
    /// A floating, movable window (egui::Window).
    Window {
        title: String,
        id: String,
        default_size: Option<[f32; 2]>,
    },
    /// A side panel (egui::SidePanel).
    SidePanel {
        title: String,
        id: String,
        side: PanelSide,
    },
    /// A full-screen overlay (e.g., 3D view, experiment).
    FullScreen {
        title: String,
        id: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PanelSide {
    Left,
    Right,
}

/// Content renderer for a UI panel — called each frame.
pub type PanelRenderer = Arc<dyn Fn(&Context) + Send + Sync>;

// ── Plugin Context ────────────────────────────────────────────────

/// Host services exposed to plugins during initialization and runtime.
///
/// Plugins use the context to register UI elements, access the document,
/// and interact with the host application.
pub struct PluginContext {
    /// Toolbar buttons registered by plugins (consumed by the host).
    pub toolbar_actions: Vec<ToolbarAction>,
    /// UI panels registered by plugins.
    pub ui_panels: Vec<(UiPanel, PanelRenderer)>,
    /// Reference to the current document (if any is open).
    pub document: Option<CoursewareDoc>,
    /// Log messages emitted by the plugin.
    pub log_messages: Vec<String>,
}

impl PluginContext {
    /// Create a new empty context.
    pub fn new() -> Self {
        Self {
            toolbar_actions: Vec::new(),
            ui_panels: Vec::new(),
            document: None,
            log_messages: Vec::new(),
        }
    }

    /// Create a context with an existing document.
    pub fn with_document(doc: CoursewareDoc) -> Self {
        Self {
            document: Some(doc),
            ..Self::new()
        }
    }

    // ── Toolbar ───────────────────────────────────────────────

    /// Register a toolbar button. When clicked, `on_click` is called
    /// with the egui context so the plugin can open windows/popups.
    pub fn add_toolbar_button(
        &mut self,
        id: &str,
        label: &str,
        tooltip: &str,
        on_click: impl Fn(&Context) + Send + Sync + 'static,
    ) {
        self.toolbar_actions.push(ToolbarAction {
            id: id.to_string(),
            label: label.to_string(),
            tooltip: tooltip.to_string(),
            on_click: Arc::new(on_click),
        });
    }

    // ── UI Panels ─────────────────────────────────────────────

    /// Register a floating window panel.
    pub fn add_window(
        &mut self,
        id: &str,
        title: &str,
        renderer: impl Fn(&Context) + Send + Sync + 'static,
    ) {
        self.ui_panels.push((
            UiPanel::Window {
                id: id.to_string(),
                title: title.to_string(),
                default_size: None,
            },
            Arc::new(renderer),
        ));
    }

    /// Register a side panel.
    pub fn add_side_panel(
        &mut self,
        id: &str,
        title: &str,
        side: PanelSide,
        renderer: impl Fn(&Context) + Send + Sync + 'static,
    ) {
        self.ui_panels.push((
            UiPanel::SidePanel {
                id: id.to_string(),
                title: title.to_string(),
                side,
            },
            Arc::new(renderer),
        ));
    }

    /// Register a full-screen overlay.
    pub fn add_fullscreen(
        &mut self,
        id: &str,
        title: &str,
        renderer: impl Fn(&Context) + Send + Sync + 'static,
    ) {
        self.ui_panels.push((
            UiPanel::FullScreen {
                id: id.to_string(),
                title: title.to_string(),
            },
            Arc::new(renderer),
        ));
    }

    // ── Document ──────────────────────────────────────────────

    /// Get a reference to the current document, if any.
    pub fn document(&self) -> Option<&CoursewareDoc> {
        self.document.as_ref()
    }

    /// Get a mutable reference to the current document.
    pub fn document_mut(&mut self) -> Option<&mut CoursewareDoc> {
        self.document.as_mut()
    }

    // ── Logging ───────────────────────────────────────────────

    /// Emit a log message visible in the host's log view.
    pub fn log(&mut self, msg: &str) {
        log::info!("[plugin] {msg}");
        self.log_messages.push(msg.to_string());
    }
}

impl Default for PluginContext {
    fn default() -> Self {
        Self::new()
    }
}

// ── DrafftinkPlugin Trait ─────────────────────────────────────────

/// The core trait that every dynamic plugin must implement.
///
/// Unlike the simpler `Plugin` trait (which is focused on file importers),
/// `DrafftinkPlugin` gives plugins full access to the host UI and document
/// model via `PluginContext`.
///
/// # Example
///
/// ```ignore
/// #[export_plugin]
/// impl DrafftinkPlugin for MathModule {
///     fn name(&self) -> &'static str { "math" }
///     fn version(&self) -> &'static str { "1.0.0" }
///
///     fn initialize(&mut self, ctx: &mut PluginContext) {
///         ctx.add_toolbar_button("math_geo", "📐 几何", "打开几何工具", |egui_ctx| {
///             // open a window...
///         });
///     }
///
///     fn shutdown(&mut self) {
///         // cleanup
///     }
/// }
/// ```
pub trait DrafftinkPlugin: Send + Sync {
    /// Human-readable plugin name.
    fn name(&self) -> &'static str;

    /// Semantic version string.
    fn version(&self) -> &'static str;

    /// Called once when the plugin is loaded. Use `ctx` to register
    /// toolbar buttons, UI panels, and access the document.
    fn initialize(&mut self, ctx: &mut PluginContext);

    /// Called when the plugin is about to be unloaded. Clean up
    /// resources, save state, etc.
    fn shutdown(&mut self);
}

// ── FFI Entry Point Type ──────────────────────────────────────────

/// Signature of the `create_plugin` symbol exported by every cdylib plugin.
///
/// Generated automatically by the `#[export_plugin]` proc-macro.
#[allow(improper_ctypes_definitions)]
pub type DrafftinkPluginEntryFn = extern "C" fn() -> *mut dyn DrafftinkPlugin;

// ── Dummy plugin for testing ──────────────────────────────────────

/// A minimal plugin implementation for testing the loader.
pub struct DummyPlugin {
    pub name_str: &'static str,
    pub version_str: &'static str,
    pub initialized: bool,
}

impl DummyPlugin {
    pub fn new(name: &'static str, version: &'static str) -> Self {
        Self {
            name_str: name,
            version_str: version,
            initialized: false,
        }
    }
}

impl Default for DummyPlugin {
    fn default() -> Self {
        Self::new("dummy", "0.0.0")
    }
}

impl DrafftinkPlugin for DummyPlugin {
    fn name(&self) -> &'static str {
        self.name_str
    }
    fn version(&self) -> &'static str {
        self.version_str
    }
    fn initialize(&mut self, _ctx: &mut PluginContext) {
        self.initialized = true;
    }
    fn shutdown(&mut self) {
        self.initialized = false;
    }
}