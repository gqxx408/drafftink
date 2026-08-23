//! # SeewoClass Plugin API — 稳定的 C ABI 契约
//!
//! 本 crate 定义宿主（host）与插件（plugin，编译为 `.dll`/`.so`/`.dylib`）之间跨 FFI 边界的
//! 稳定二进制接口。所有跨越边界的类型均为 `#[repr(C)]`，数据通过 `*const u8` / `*mut u8` + 长度交换。
//!
//! ## ABI 稳定性规则（破坏性变更 = 必须 bump 版本）
//!
//! 1. **禁止改变既有字段的顺序、类型或大小**：`#[repr(C)]` 保证按声明顺序布局，插件与宿主
//!    必须以完全相同的字段顺序编译。新增字段只允许**追加到结构体末尾**，旧字段不得删除或重排。
//! 2. **枚举必须是 `repr(C)` 且显式判别式**：[`PluginResult`] 使用 `Ok=0, Err=1, NotSupported=2`，
//!    新增变体只能追加到尾部并赋予新的显式值，不得复用或重排既有值。
//! 3. **版本协商**：宿主以 [`PLUGIN_API_VERSION`] 校验插件 `_plugin_meta().api_version`；
//!    不匹配直接拒绝加载（见 `seewo-plugin-loader`）。**任何破坏上述布局/语义的修改都视为
//!    破坏性变更，必须将 `PLUGIN_API_VERSION` 加 1**，由加载侧实现对应的多版本兼容或拒绝。
//! 4. **字符串与切片**：所有字符串以 `(ptr, len)` 传入，编码为 UTF-8，**不要求 NUL 结尾**；
//!    长度由调用方提供，接收方不得越界读取。
//! 5. **所有权边界**：插件分配的缓冲区不得由宿主释放，反之亦然；输出缓冲区由宿主预分配并传入
//!    `output`/`output_len`，插件写入后通过 `output_len` 回填实际长度，且不得超过预分配容量。
//!
//! ## 跨边界恐慌（panic）
//!
//! FFI 边界**绝不**允许 Rust panic 跨越（会导致未定义行为）。宿主在 `LoadedPlugin::execute`
//! 中用 `catch_unwind` 捕获插件 panic 并转为 [`PluginResult::Err`]；插件侧应配置
//! `panic = "abort"` 或在每个导出函数内自行 `catch_unwind`，确保不向宿主 unwind。
//!
//! ## 导出符号约定
//!
//! 插件必须导出 `_plugin_meta`、`_plugin_create`、`_plugin_destroy`、`_plugin_execute`
//! 四个 `extern "C"` 符号（签名见对应类型别名）。加载失败时宿主返回 `Err(String)` 而非 panic。

// ----- Metadata ---------------------------------------------------------

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PluginMeta {
    pub api_version: u32,
    pub name: *const u8,
    pub name_len: u32,
    pub desc: *const u8,
    pub desc_len: u32,
    pub version: *const u8,
    pub version_len: u32,
}

pub const PLUGIN_API_VERSION: u32 = 1;

// ----- Result codes -----------------------------------------------------

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginResult { Ok = 0, Err = 1, NotSupported = 2 }

// ----- Context passed from host to plugin --------------------------------

/// Callbacks the plugin can use to interact with the host.
#[repr(C)]
#[derive(Clone)]
pub struct PluginContext {
    /// Log a message (level: 0=trace 1=debug 2=info 3=warn 4=error).
    pub log_fn: unsafe extern "C" fn(level: u8, msg: *const u8, len: u32),
    /// Show a toast notification in the host UI.
    pub toast_fn: unsafe extern "C" fn(msg: *const u8, len: u32),
    /// Opaque pointer to the host app state (egui Context etc.).
    pub host_data: *mut std::ffi::c_void,
}

// ----- C ABI function types ----------------------------------------------

pub type PluginHandle = *mut std::ffi::c_void;
pub type PluginMetaFn = unsafe extern "C" fn() -> PluginMeta;
pub type PluginCreateFn = unsafe extern "C" fn(ctx: *const PluginContext) -> PluginHandle;
pub type PluginDestroyFn = unsafe extern "C" fn(handle: PluginHandle);
pub type PluginExecuteFn = unsafe extern "C" fn(
    handle: PluginHandle,
    action: *const u8, action_len: u32,
    input: *const u8, input_len: u32,
    output: *mut u8, output_len: *mut u32,
) -> PluginResult;

// ----- Helpers for plugin authors -----------------------------------------

/// # Safety
/// `ptr` 必须指向长度为 `len` 的有效 UTF-8 内存，且该内存在本次调用期间保持有效；
/// 若指针为空、越界或非 UTF-8，则行为未定义。
pub unsafe fn read_str(ptr: *const u8, len: u32) -> &'static str {
    std::str::from_utf8(std::slice::from_raw_parts(ptr, len as usize)).unwrap_or("")
}

/// Install a simple `log` backend that forwards all log calls to the host
/// via `PluginContext::log_fn`.  Call this once at plugin initialisation.
pub fn install_host_logger(ctx: &PluginContext) {
    struct ProxyLogger {
        log_fn: unsafe extern "C" fn(level: u8, msg: *const u8, len: u32),
    }
    unsafe impl Send for ProxyLogger {}
    unsafe impl Sync for ProxyLogger {}

    impl log::Log for ProxyLogger {
        fn enabled(&self, _: &log::Metadata) -> bool { true }
        fn log(&self, record: &log::Record) {
            let lvl = record.level() as u8;
            let msg = format!("{}", record.args());
            unsafe { (self.log_fn)(lvl, msg.as_ptr(), msg.len() as u32); }
        }
        fn flush(&self) {}
    }
    let _ = log::set_boxed_logger(Box::new(ProxyLogger { log_fn: ctx.log_fn }));
    log::set_max_level(log::LevelFilter::Debug);
}
