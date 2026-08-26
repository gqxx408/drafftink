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
            name: name.as_ptr() as *const u8,
            name_len: name.to_bytes().len() as u32,
            desc: desc.as_ptr() as *const u8,
            desc_len: desc.to_bytes().len() as u32,
            version: ver.as_ptr() as *const u8,
            version_len: ver.to_bytes().len() as u32,
        }
    }

    unsafe fn execute(
        action: *const u8,
        action_len: u32,
        input: *const u8,
        input_len: u32,
        output: *mut u8,
        output_len: *mut u32,
    ) -> PluginResult {
        let action = unsafe { seewo_plugin_api::read_str(action, action_len) };
        if action != "import" {
            return PluginResult::NotSupported;
        }

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
/// # Safety
///
/// 由宿主的插件装载器通过 C ABI 调用，仅允许传入宿主编译期约定的有效元数据。
pub unsafe extern "C" fn _plugin_meta() -> PluginMeta {
    EnbxImporter::meta()
}

#[no_mangle]
/// # Safety
///
/// 宿主编译期调用此函数初始化插件实例；`ctx` 为空指针时安全返回一个悬空句柄。
pub unsafe extern "C" fn _plugin_create(ctx: *const PluginContext) -> *mut std::ffi::c_void {
    if !ctx.is_null() {
        seewo_plugin_api::install_host_logger(&*ctx);
    }
    Box::into_raw(Box::new(EnbxImporter)) as _
}

#[no_mangle]
/// # Safety
///
/// 只能以 `_plugin_create` 返回的合法句柄调用一次；重复或越界指针会造成未定义行为。
pub unsafe extern "C" fn _plugin_destroy(h: *mut std::ffi::c_void) {
    if !h.is_null() {
        drop(Box::from_raw(h as *mut EnbxImporter));
    }
}

#[no_mangle]
/// # Safety
///
/// 所有指针（`h`、`a`、`i`、`o`）必须指向主机分配的合法内存，`ol` 指向有效长度槽；
/// 缓冲区大小由 `al`/`il`/`*ol` 约定，越界访问由调用方负责保证。
pub unsafe extern "C" fn _plugin_execute(
    h: *mut std::ffi::c_void,
    a: *const u8,
    al: u32,
    i: *const u8,
    il: u32,
    o: *mut u8,
    ol: *mut u32,
) -> PluginResult {
    if h.is_null() {
        return PluginResult::Err;
    }
    EnbxImporter::execute(a, al, i, il, o, ol)
}
