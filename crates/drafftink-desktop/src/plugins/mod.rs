//! Plugin system for the desktop app.
//!
//! Provides `PluginManager` for loading, unloading, and toggling plugins
//! (DLL or WASM).  The actual dynamic loading is stubbed for the MVP —
//! plugins are registered by metadata only.

pub mod manager;
