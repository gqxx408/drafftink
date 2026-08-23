//! math_module — drafftink dynamic plugin providing geometry tools.
//!
//! Compiled as `math_module.dll` (cdylib). The host loads it via
//! libloading, calls `create_plugin()`, and receives a
//! `Box<dyn DrafftinkPlugin>`.
//!
//! This plugin demonstrates:
//!   - Registering toolbar buttons (📐 几何)
//!   - Adding a floating UI panel (坐标计算器)
//!   - Registering custom GeometryElement types
//!   - Using the `#[export_plugin]` proc-macro

pub mod geometry;

use drafftink_core::plugin::{DrafftinkPlugin, PluginContext};
use drafftink_plugin_macros::export_plugin;

// ── MathModule ────────────────────────────────────────────────────

/// The root plugin struct. Must implement `Default` for `#[export_plugin]`.
#[derive(Default)]
pub struct MathModule {
    /// Whether the geometry panel is currently visible.
    pub geometry_panel_open: bool,
}

impl MathModule {
    /// Create a new MathModule with default state.
    pub fn new() -> Self {
        Self {
            geometry_panel_open: false,
        }
    }
}

// ── DrafftinkPlugin Implementation ────────────────────────────────

#[export_plugin]
impl DrafftinkPlugin for MathModule {
    fn name(&self) -> &'static str {
        "math"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn initialize(&mut self, ctx: &mut PluginContext) {
        log::info!("[math_module] Initializing v{}", self.version());

        // ── 1. Register a toolbar button ──────────────────────
        ctx.add_toolbar_button(
            "math_geometry",
            "📐 几何",
            "打开几何工具面板",
            |_egui_ctx| {
                // The toolbar button click is handled by the host.
                // The host inspects the clicked action ID and toggles
                // the corresponding panel.
                log::info!("[math_module] Geometry toolbar button clicked");
            },
        );

        // ── 2. Register a side panel for coordinate calculator ─
        ctx.add_side_panel(
            "math_coord",
            "坐标计算器",
            drafftink_core::plugin::PanelSide::Right,
            |_egui_ctx| {
                // The actual rendering happens in the host's egui loop.
                // This closure is a placeholder; the host calls
                // `panel_renderer(ctx)` each frame.
                // Real implementation would use egui widgets here.
            },
        );

        // ── 3. Register a floating window for geometry tools ──
        ctx.add_window(
            "math_geometry_window",
            "📐 几何工具",
            |_egui_ctx| {
                // Geometry tool window content rendered by the host.
            },
        );

        // ── 4. Log registration ───────────────────────────────
        ctx.log(&format!(
            "[math_module] Registered {} toolbar actions, {} panels",
            ctx.toolbar_actions.len(),
            ctx.ui_panels.len(),
        ));
    }

    fn shutdown(&mut self) {
        log::info!("[math_module] Shutting down");
        self.geometry_panel_open = false;
    }
}