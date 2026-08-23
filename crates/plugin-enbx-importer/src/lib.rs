//! ENBX/ENPX import plugin (cdylib).

use seewo_plugin_api::{PluginContext, PluginMeta, PluginResult, PLUGIN_API_VERSION};
use std::path::PathBuf;

struct EnbxImporter;

impl EnbxImporter {
    fn meta() -> PluginMeta {
        let name = c"enbx_importer";
        let desc = c"Import .enbx/.enpx courseware files";
        let ver = c"0.1.0";
        PluginMeta {
            api_version: PLUGIN_API_VERSION,
            name: name.as_ptr() as *const u8, name_len: name.to_bytes().len() as u32,
            desc: desc.as_ptr() as *const u8, desc_len: desc.to_bytes().len() as u32,
            version: ver.as_ptr() as *const u8, version_len: ver.to_bytes().len() as u32,
        }
    }

    unsafe fn execute(
        action: *const u8, action_len: u32,
        input: *const u8, input_len: u32,
        output: *mut u8, output_len: *mut u32,
    ) -> PluginResult {
        let action = unsafe { seewo_plugin_api::read_str(action, action_len) };
        if action != "import" { return PluginResult::NotSupported; }

        let path_str = unsafe { seewo_plugin_api::read_str(input, input_len) };
        let path = PathBuf::from(path_str);

        match enbx_importer::import_enbx(&path, None) {
            Ok((doc, _)) => {
                let data = serde_json::to_vec(&doc).unwrap_or_default();
                let len = data.len() as u32;
                if len > *output_len {
                    *output_len = len;
                    return PluginResult::Err;
                }
                *output_len = len;
                unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), output, data.len()) };
                PluginResult::Ok
            }
            Err(_) => PluginResult::Err,
        }
    }
}

// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn _plugin_meta() -> PluginMeta { EnbxImporter::meta() }

#[no_mangle]
pub unsafe extern "C" fn _plugin_create(ctx: *const PluginContext) -> *mut std::ffi::c_void {
    if !ctx.is_null() {
        seewo_plugin_api::install_host_logger(&*ctx);
    }
    Box::into_raw(Box::new(EnbxImporter)) as _
}

#[no_mangle]
pub unsafe extern "C" fn _plugin_destroy(h: *mut std::ffi::c_void) {
    if !h.is_null() { drop(Box::from_raw(h as *mut EnbxImporter)); }
}

#[no_mangle]
pub unsafe extern "C" fn _plugin_execute(
    h: *mut std::ffi::c_void,
    a: *const u8, al: u32,
    i: *const u8, il: u32,
    o: *mut u8, ol: *mut u32,
) -> PluginResult {
    if h.is_null() { return PluginResult::Err; }
    EnbxImporter::execute(a, al, i, il, o, ol)
}
