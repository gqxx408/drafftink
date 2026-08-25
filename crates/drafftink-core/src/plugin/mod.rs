//! Plugin system for drafftink — dynamic loading of format importers and tools.
//!
//! Architecture:
//!  - `api.rs`              — trait definitions shared between host and plugins
//!  - `drafftink_plugin.rs` — enhanced DrafftinkPlugin trait + PluginContext (UI, toolbar, doc)
//!  - `loader.rs`           — PluginManager: scan, load, unload cdylibs via libloading
//!  - `sandbox.rs`          — permission model + error isolation
//!  - `signing.rs`          — Ed25519 signature verification
//!  - `audit.rs`            — JSONL audit trail

pub mod api;
pub mod audit;
pub mod drafftink_plugin;
pub mod loader;
pub mod sandbox;
pub mod signing;

pub use api::{
    FileImporter, Permission, Plugin, PluginContext as BasePluginContext, PluginManifest,
};
pub use drafftink_plugin::{
    DrafftinkPlugin, DrafftinkPluginEntryFn, DummyPlugin, PanelSide, PluginContext, ToolbarAction,
    UiPanel,
};
pub use loader::{DrafftinkPluginLoader, LoadedDrafftinkPlugin, PluginManager};
pub use signing::SigStatus;

#[cfg(test)]
mod security_test;
