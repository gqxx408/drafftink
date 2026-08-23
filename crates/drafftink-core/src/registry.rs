//! Plugin trait and registry — replaces EasiNote's `[StartupTask]` attribute.
//!
//! # Design
//!
//! In the original C# codebase, Seewo used `[StartupTask]` attributes on
//! static methods, discovered via reflection at startup.  Rust has no
//! runtime reflection, so we use a **declarative macro** that registers
//! plugin startup functions into a `PluginRegistry` during `main()`.
//!
//! ```text
//!  main() {
//!      let mut registry = PluginRegistry::new();
//!      register_plugin!(registry, GeometryPlugin);
//!      register_plugin!(registry, FormulaPlugin);
//!      // ...
//!      let mut ctx = BoardContext::new();
//!      registry.run_startup(&mut ctx);
//!  }
//! ```
//!
//! # Trait vs. Enum Dispatch
//!
//! The `Plugin` trait is object-safe, so `PluginRegistry` stores
//! `Box<dyn Plugin>`.  This allows plugins to be added dynamically
//! (e.g. loaded from cdylibs) as well as statically.

use crate::context::BoardContext;

// ---------------------------------------------------------------------------
// Plugin trait
// ---------------------------------------------------------------------------

/// Core trait for every drafftink plugin.
///
/// A plugin receives a `&mut BoardContext` during startup and can:
/// - Enqueue initial elements
/// - Register toolbar actions
/// - Set up event handlers
/// - Modify the board state
///
/// # Example
///
/// ```ignore
/// pub struct GeometryPlugin;
///
/// impl Default for GeometryPlugin {
///     fn default() -> Self { Self }
/// }
///
/// impl Plugin for GeometryPlugin {
///     fn name(&self) -> &'static str { "geometry" }
///     fn version(&self) -> &'static str { "0.1.0" }
///
///     fn startup(&mut self, ctx: &mut BoardContext) {
///         // Add a default geometry element
///         let base = BaseElement { ... };
///         ctx.add_element(ElementData::geometry(base, serde_json::json!({})));
///     }
/// }
/// ```
///
/// In `main()`:
/// ```ignore
/// register_plugin!(registry, GeometryPlugin);
/// ```
pub trait Plugin: Send + Sync {
    /// Human-readable plugin name.
    fn name(&self) -> &'static str;

    /// Semantic version string.
    fn version(&self) -> &'static str;

    /// Called once during application startup.
    ///
    /// Receives a mutable reference to the `BoardContext`, allowing the
    /// plugin to enqueue commands, add elements, or modify state.
    fn startup(&mut self, ctx: &mut BoardContext);

    /// Called when the plugin is being unloaded.
    /// Override to clean up resources.
    fn shutdown(&mut self) {}
}

// ---------------------------------------------------------------------------
// PluginRegistry
// ---------------------------------------------------------------------------

/// A central registry that collects plugins and runs their startup tasks.
///
/// Created in `main()` and populated via `register_plugin!` or direct
/// `register()` calls.  No global statics are used — the registry is a
/// plain struct owned by the application.
///
/// # Example
///
/// ```ignore
/// let mut registry = PluginRegistry::new();
/// register_plugin!(registry, GeometryPlugin);
/// register_plugin!(registry, FormulaPlugin);
///
/// let mut ctx = BoardContext::new();
/// registry.run_startup(&mut ctx);
/// ```
pub struct PluginRegistry {
    plugins: Vec<Box<dyn Plugin>>,
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    /// Register a plugin instance.
    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        log::info!("[registry] Registered plugin: {} v{}", plugin.name(), plugin.version());
        self.plugins.push(plugin);
    }

    /// Number of registered plugins.
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    /// Run `startup()` on all registered plugins, in registration order.
    ///
    /// Each plugin receives a `&mut BoardContext` and may enqueue commands
    /// or modify state.  After all startups have run, the caller should
    /// call `ctx.process_commands()` to flush the command queue.
    pub fn run_startup(&mut self, ctx: &mut BoardContext) {
        for plugin in &mut self.plugins {
            log::debug!("[registry] Starting plugin: {}", plugin.name());
            plugin.startup(ctx);
        }
    }

    /// Run `shutdown()` on all registered plugins, in reverse order.
    pub fn run_shutdown(&mut self) {
        for plugin in self.plugins.iter_mut().rev() {
            log::debug!("[registry] Shutting down plugin: {}", plugin.name());
            plugin.shutdown();
        }
    }

    /// Borrow a plugin by name.
    pub fn get(&self, name: &str) -> Option<&dyn Plugin> {
        self.plugins
            .iter()
            .find(|p| p.name() == name)
            .map(|p| p.as_ref())
    }

    /// List all registered plugin names.
    pub fn plugin_names(&self) -> Vec<&'static str> {
        self.plugins.iter().map(|p| p.name()).collect()
    }
}

// ---------------------------------------------------------------------------
// register_plugin! macro
// ---------------------------------------------------------------------------

/// Declarative macro for registering a plugin into a `PluginRegistry`.
///
/// This simulates the automatic discovery of `[StartupTask]` attributes
/// in the original C# codebase, without requiring runtime reflection.
///
/// # Usage
///
/// ```ignore
/// register_plugin!(registry, GeometryPlugin);
/// ```
///
/// Expands to:
/// ```ignore
/// registry.register(Box::new(GeometryPlugin::default()));
/// ```
///
/// The plugin type must implement both `Default` and `Plugin`.
/// For plugins that require custom construction, call
/// `registry.register(Box::new(my_plugin))` directly.
#[macro_export]
macro_rules! register_plugin {
    ($registry:expr, $plugin_type:ty) => {
        $registry.register(Box::new(<$plugin_type>::default()));
    };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::ElementData;
    use crate::model::{BaseElement, ShapeElement, ShapeType};

    // ── Test plugin implementations ─────────────────────────────────

    #[derive(Default)]
struct TestPluginA {
        started: bool,
        shutdown: bool,
    }

    

    impl Plugin for TestPluginA {
        fn name(&self) -> &'static str {
            "plugin_a"
        }
        fn version(&self) -> &'static str {
            "1.0.0"
        }
        fn startup(&mut self, ctx: &mut BoardContext) {
            self.started = true;
            // Add a test element
            ctx.add_element(ElementData::Shape(ShapeElement {
                base: BaseElement::default(),
                shape_type: ShapeType::Rectangle,
                has_start_arrow: false,
                has_end_arrow: false,
                scale_y: 0.0,
            }));
        }
        fn shutdown(&mut self) {
            self.shutdown = true;
        }
    }

    struct TestPluginB;

    impl Default for TestPluginB {
        fn default() -> Self {
            Self
        }
    }

    impl Plugin for TestPluginB {
        fn name(&self) -> &'static str {
            "plugin_b"
        }
        fn version(&self) -> &'static str {
            "0.2.0"
        }
        fn startup(&mut self, _ctx: &mut BoardContext) {}
    }

    // ── Tests ───────────────────────────────────────────────────────

    #[test]
    fn registry_empty() {
        let registry = PluginRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn registry_register_and_count() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(TestPluginA::default()));
        registry.register(Box::new(TestPluginB));
        assert_eq!(registry.len(), 2);
        assert!(!registry.is_empty());
    }

    #[test]
    fn registry_plugin_names() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(TestPluginA::default()));
        registry.register(Box::new(TestPluginB));

        let names = registry.plugin_names();
        assert_eq!(names, vec!["plugin_a", "plugin_b"]);
    }

    #[test]
    fn registry_get_by_name() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(TestPluginA::default()));
        registry.register(Box::new(TestPluginB));

        assert!(registry.get("plugin_a").is_some());
        assert!(registry.get("plugin_b").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn registry_run_startup() {
        let mut registry = PluginRegistry::new();
        let plugin = TestPluginA::default();
        registry.register(Box::new(plugin));

        let mut ctx = BoardContext::new();
        registry.run_startup(&mut ctx);

        // The plugin should have enqueued an AddElement command
        assert_eq!(ctx.pending_command_count(), 1);
        ctx.process_commands();
        assert_eq!(ctx.elements().len(), 1);
    }

    #[test]
    fn registry_run_shutdown() {
        let mut registry = PluginRegistry::new();
        let plugin = TestPluginA {
            started: true,
            ..Default::default()
        };
        registry.register(Box::new(plugin));

        registry.run_shutdown();
        // We can't inspect the plugin directly after it's boxed,
        // but we can verify no panic occurs.
    }

    #[test]
    fn register_plugin_macro_type_form() {
        let mut registry = PluginRegistry::new();
        register_plugin!(registry, TestPluginA);
        register_plugin!(registry, TestPluginB);

        assert_eq!(registry.len(), 2);
        assert_eq!(registry.plugin_names(), vec!["plugin_a", "plugin_b"]);
    }

    #[test]
    fn register_plugin_direct_instance() {
        let mut registry = PluginRegistry::new();
        let custom = TestPluginA {
            started: true,
            shutdown: false,
        };
        registry.register(Box::new(custom));

        assert_eq!(registry.len(), 1);
        assert_eq!(registry.get("plugin_a").unwrap().version(), "1.0.0");
    }

    #[test]
    fn registry_no_global_state() {
        // Two registries must be fully independent.
        let mut reg_a = PluginRegistry::new();
        let reg_b = PluginRegistry::new();

        reg_a.register(Box::new(TestPluginA::default()));

        assert_eq!(reg_a.len(), 1);
        assert_eq!(reg_b.len(), 0);
    }

    #[test]
    fn registry_multiple_plugins_startup_order() {
        let mut registry = PluginRegistry::new();
        register_plugin!(registry, TestPluginA);
        register_plugin!(registry, TestPluginB);

        let mut ctx = BoardContext::new();
        registry.run_startup(&mut ctx);

        // Only TestPluginA adds an element
        assert_eq!(ctx.pending_command_count(), 1);
    }
}
