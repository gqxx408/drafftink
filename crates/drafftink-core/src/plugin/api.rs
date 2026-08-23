//! Plugin API — trait definitions shared between drafftink host and plugins.
//!
//! Plugins are compiled as cdylibs and loaded at runtime via libloading.
//! ABI stability is maintained by:
//!   1. Using `#[repr(C)]` for all FFI-crossing structs
//!   2. Keeping trait objects behind `Box<dyn Trait>`
//!   3. Allocating/freeing on the same side of the FFI boundary

use crate::model::CoursewareDoc;
use std::fmt;

// ── Plugin identity ──────────────────────────────────────────────

/// Metadata returned by every plugin at load time.
#[derive(Debug, Clone)]
pub struct PluginManifest {
    pub name: String,        // e.g. "format_enbx"
    pub version: String,     // "0.1.0"
    pub description: String, // "希沃 .enbx 文件导入器"
    pub author: String,      // "drafftink team"
    pub permissions: Vec<Permission>,
}

// ── Permission model ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Permission {
    ReadFiles,
    WriteFiles,
    NetworkAccess,
    FullScreen,
    SystemInfo,
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadFiles => write!(f, "读取文件"),
            Self::WriteFiles => write!(f, "写入文件"),
            Self::NetworkAccess => write!(f, "网络访问"),
            Self::FullScreen => write!(f, "全屏渲染"),
            Self::SystemInfo => write!(f, "系统信息"),
        }
    }
}

/// Signature verification outcome.
#[derive(Debug, Clone, PartialEq)]
pub enum SignatureStatus {
    Verified,    // official or trusted key
    SelfSigned,  // community developer, key present
    Untrusted,   // signature mismatch
    NoSignature, // no signature field
}

/// Services the host provides to plugins.
pub trait PluginContext: Send + Sync {
    fn log(&self, level: &str, msg: &str);
    fn read_file(&self, path: &str) -> Result<Vec<u8>, String>;
    fn system_info(&self) -> Vec<(String, String)>;
}

/// Minimal concrete context used before real host integration.
pub struct DummyContext;

impl PluginContext for DummyContext {
    fn log(&self, level: &str, msg: &str) {
        // Route through the log crate so file-based loggers capture it.
        match level {
            "error" => log::error!("{msg}"),
            "warn" => log::warn!("{msg}"),
            "debug" => log::debug!("{msg}"),
            _ => log::info!("{msg}"),
        }
    }
    fn read_file(&self, path: &str) -> Result<Vec<u8>, String> {
        std::fs::read(path).map_err(|e| e.to_string())
    }
    fn system_info(&self) -> Vec<(String, String)> {
        vec![("os".into(), std::env::consts::OS.into())]
    }
}

// ── File importer trait ──────────────────────────────────────────

/// A plugin that can import foreign file formats into CoursewareDoc.
pub trait FileImporter: Send + Sync {
    fn supported_extensions(&self) -> Vec<String>;
    fn can_import(&self, data: &[u8]) -> bool;
    fn import(&self, data: &[u8], ctx: &dyn PluginContext) -> Result<CoursewareDoc, String>;
}

// ── Plugin entry trait ───────────────────────────────────────────

/// Every cdylib plugin must implement this and expose it via
///   `pub extern "C" fn drafftink_plugin_entry() -> *mut dyn Plugin`.
pub trait Plugin: Send + Sync {
    fn manifest(&self) -> &PluginManifest;
    fn file_importer(&self) -> Option<&dyn FileImporter> { None }
    fn on_load(&self, _ctx: &dyn PluginContext) {}
    fn on_unload(&self) {}
}

// ── FFI entry point type ─────────────────────────────────────────

/// Signature every cdylib must export.
///
/// ```ignore
/// #[no_mangle]
/// pub extern "C" fn drafftink_plugin_entry() -> *mut dyn Plugin {
///     Box::into_raw(Box::new(MyPlugin))
/// }
/// ```
#[allow(improper_ctypes_definitions)]
pub type PluginEntryFn = extern "C" fn() -> *mut dyn crate::plugin::api::Plugin;
