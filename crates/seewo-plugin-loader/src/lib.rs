//! Plugin loader — discovers, loads, and manages `.dll` plugins.
//!
//! Wraps `libloading` with ABI version checks and panic isolation.

use seewo_plugin_api::{
    PluginContext, PluginExecuteFn, PluginMeta, PluginResult, PLUGIN_API_VERSION,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

// ----- Public API --------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PluginMetaOwned {
    pub api_version: u32,
    pub name: String,
    pub desc: String,
    pub version: String,
}

/// A loaded plugin with lifecycle management.
pub struct LoadedPlugin {
    _lib: Arc<libloading::Library>,
    pub meta: PluginMetaOwned,
    handle: *mut std::ffi::c_void,
    destroy: unsafe extern "C" fn(*mut std::ffi::c_void),
    exec: PluginExecuteFn,
}

impl LoadedPlugin {
    /// Load a plugin DLL, call `_plugin_meta`, `_plugin_create(ctx)`.
    ///
    /// # Safety
    /// `path` 必须指向与宿主平台 ABI 兼容的有效插件库；`ctx` 及其指向的所有数据
    /// （含 `host_data`）必须在返回的 [`LoadedPlugin`] 生命周期内保持有效
    /// （插件可能在内部捕获 `host_data` 指针）。
    pub unsafe fn load(path: &Path, ctx: &PluginContext) -> Result<Self, String> {
        let lib = unsafe { libloading::Library::new(path) }
            .map_err(|e| format!("open: {e}"))?;
        let lib = Arc::new(lib);

        // Resolve all exports before constructing Self
        let meta_ptr = {
            let sym: libloading::Symbol<unsafe extern "C" fn() -> PluginMeta> =
                unsafe { lib.get(b"_plugin_meta") }
                    .map_err(|e| format!("_plugin_meta: {e}"))?;
            unsafe { sym() }
        };
        let meta = PluginMetaOwned {
            api_version: meta_ptr.api_version,
            name: unsafe { read_cstr(meta_ptr.name, meta_ptr.name_len) },
            desc: unsafe { read_cstr(meta_ptr.desc, meta_ptr.desc_len) },
            version: unsafe { read_cstr(meta_ptr.version, meta_ptr.version_len) },
        };
        if meta.api_version != PLUGIN_API_VERSION {
            return Err(format!("{} API v{}, host v{}", meta.name, meta.api_version, PLUGIN_API_VERSION));
        }

        let create: libloading::Symbol<
            unsafe extern "C" fn(*const PluginContext) -> *mut std::ffi::c_void,
        > = unsafe { lib.get(b"_plugin_create") }
            .map_err(|e| format!("_plugin_create: {e}"))?;
        let handle = unsafe { create(ctx as *const PluginContext) };
        if handle.is_null() {
            return Err("_plugin_create returned null".into());
        }

        let destroy: unsafe extern "C" fn(*mut std::ffi::c_void) = {
            let sym: libloading::Symbol<unsafe extern "C" fn(*mut std::ffi::c_void)> =
                unsafe { lib.get(b"_plugin_destroy") }
                    .map_err(|e| format!("_plugin_destroy: {e}"))?;
            *sym
        };

        let exec: PluginExecuteFn = {
            let sym: libloading::Symbol<PluginExecuteFn> =
                unsafe { lib.get(b"_plugin_execute") }
                    .map_err(|e| format!("_plugin_execute: {e}"))?;
            *sym
        };

        log::info!("Loaded plugin: {} v{} — {}", meta.name, meta.version, meta.desc);

        Ok(Self { _lib: lib, meta, handle, destroy, exec })
    }

    /// Execute a named action.  Panics from the plugin are caught.
    pub fn execute(&self, action: &str, input: &[u8]) -> Result<Vec<u8>, String> {
        let mut out = vec![0u8; 256 * 1024]; // 256 KB pre-alloc
        let mut out_len = out.len() as u32;

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            (self.exec)(
                self.handle,
                action.as_ptr(),
                action.len() as u32,
                input.as_ptr(),
                input.len() as u32,
                out.as_mut_ptr(),
                &mut out_len,
            )
        }));

        match result {
            Ok(PluginResult::Ok) => {
                out.truncate(out_len as usize);
                Ok(out)
            }
            Ok(PluginResult::Err) => Err(String::from_utf8_lossy(&out[..out_len as usize]).into()),
            Ok(PluginResult::NotSupported) => Err("action not supported".into()),
            Err(_) => Err("plugin panicked".into()),
        }
    }
}

impl Drop for LoadedPlugin {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { (self.destroy)(self.handle) };
            self.handle = std::ptr::null_mut();
        }
        log::info!("Unloaded plugin: {}", self.meta.name);
    }
}

// ----- Helpers -----------------------------------------------------------

pub fn discover_plugins(dir: &Path) -> Vec<PathBuf> {
    let Ok(iter) = std::fs::read_dir(dir) else { return vec![] };
    iter.flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|s| s.to_str())
                .map(|s| s == "dll" || s == "so" || s == "dylib")
                .unwrap_or(false)
        })
        .collect()
}

unsafe fn read_cstr(ptr: *const u8, len: u32) -> String {
    if ptr.is_null() || len == 0 { return String::new(); }
    String::from_utf8_lossy(std::slice::from_raw_parts(ptr, len as usize)).into()
}
