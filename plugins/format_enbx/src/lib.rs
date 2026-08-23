//! format_enbx — drafftink plugin for importing Seewo .enbx courseware.
//!
//! Compiled as `format_enbx.dll` (cdylib). The host loads it via
//! libloading, calls `drafftink_plugin_entry()`, and receives a
//! `Box<dyn Plugin>` that supplies a `FileImporter` for .enbx files.

pub mod elements;
pub mod importer;
pub mod loader;
pub mod parser;
pub mod security;

#[cfg(test)]
mod parser_test;

#[cfg(test)]
mod loader_test;

use drafftink_core::plugin::api::{
    FileImporter, Permission, Plugin, PluginContext, PluginManifest,
};

// ── Plugin entry point ────────────────────────────────────────────

/// Every cdylib plugin MUST export this symbol.
///
/// The host calls it after dlopen; we return a heap-allocated trait object.
/// The host takes ownership (calls `Box::from_raw`) and is responsible for
/// later dropping it, which triggers `Plugin::on_unload`.
#[no_mangle]
#[allow(improper_ctypes_definitions)]
pub extern "C" fn drafftink_plugin_entry() -> *mut dyn Plugin {
    Box::into_raw(Box::new(EnbxPlugin::new()))
}

// ── EnbxPlugin ────────────────────────────────────────────────────

pub struct EnbxPlugin {
    manifest: PluginManifest,
}

impl EnbxPlugin {
    fn new() -> Self {
        Self {
            manifest: PluginManifest {
                name: "format_enbx".into(),
                version: "0.1.0".into(),
                description: "希沃白板 .enbx 文件导入器".into(),
                author: "drafftink".into(),
                permissions: vec![Permission::ReadFiles],
            },
        }
    }
}

impl Plugin for EnbxPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn file_importer(&self) -> Option<&dyn FileImporter> {
        Some(&importer::EnbxImporter)
    }

    fn on_load(&self, ctx: &dyn PluginContext) {
        ctx.log("info", "[format_enbx] Loaded");
    }

    fn on_unload(&self) {
        log::info!("[format_enbx] Unloaded");
    }
}
