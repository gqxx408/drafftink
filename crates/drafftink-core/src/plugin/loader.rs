//! Dynamic library loader — discovers and loads drafftink cdylib plugins.
//!
//! ## ⚠️ 安全：生产环境必须配置 `trusted_key`
//!
//! 本加载器支持运行时加载原生动态库（DLL/so/dylib），属于最高风险操作。
//! **生产环境必须通过 [`PluginManager::with_trusted_key`] /
//! [`DrafftinkPluginLoader::with_trusted_key`] 配置可信 Ed25519 公钥**，
//! 未通过签名校验的插件将被拒绝加载（fail-closed），绝不 `dlopen`。
//!
//! 若未配置 `trusted_key`：
//! - `dev_mode = true`：仅作本地开发便利，按 dev 信任列表（manifest.author）放行；
//! - `dev_mode = false`：插件系统不可用，拒绝加载任何未签名插件。
//!
//! 切勿在未配置 `trusted_key` 且非开发模式的情况下加载来源不可信的插件，
//! 否则等同于允许任意原生代码执行（RCE）。

use libloading::{Library, Symbol};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::plugin::api::{Plugin, PluginContext, PluginEntryFn, PluginManifest, SignatureStatus};
use crate::plugin::audit::AuditLogger;

/// A loaded plugin with its library handle.
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    /// Kept alive so the library stays loaded; drop → dlclose.
    _lib: Library,
    pub plugin: Box<dyn Plugin>,
}

/// Manages plugin discovery, loading, unloading, and permissions.
pub struct PluginManager {
    plugins_dir: PathBuf,
    loaded: Vec<LoadedPlugin>,
    #[allow(dead_code)]
    permissions: HashMap<String, Vec<super::api::Permission>>,
    /// Trusted Ed25519 verifying key for official plugins.
    trusted_key: Option<[u8; 32]>,
    /// 开发模式：允许 dev 信任列表（manifest.author）作为签名校验的降级替代。
    /// 仅用于本地开发；生产环境必须配置 `trusted_key`，否则插件系统不可用。
    dev_mode: bool,
}

impl PluginManager {
    pub fn new(plugins_dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&plugins_dir);
        Self {
            plugins_dir,
            loaded: Vec::new(),
            permissions: HashMap::new(),
            trusted_key: None,
            dev_mode: false,
        }
    }

    /// Set the trusted public key for official plugin verification.
    pub fn with_trusted_key(mut self, key: [u8; 32]) -> Self {
        self.trusted_key = Some(key);
        self
    }

    /// 启用开发模式：在未配置 `trusted_key` 时，允许通过 dev 信任列表
    /// （`allowed_devs`）放行插件。生产环境切勿启用。
    pub fn with_dev_mode(mut self, dev_mode: bool) -> Self {
        self.dev_mode = dev_mode;
        self
    }

    // ── Discovery ────────────────────────────────────────────────

    /// Scan the plugins directory for shared libraries.
    pub fn discover(&self) -> Vec<PathBuf> {
        let mut found = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.plugins_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    if matches!(ext, "dll" | "so" | "dylib") {
                        found.push(path);
                    }
                }
            }
        }
        found
    }

    // ── Load ─────────────────────────────────────────────────────

    /// Load and initialise a single plugin.
    ///
    /// # Safety
    /// The cdylib at `path` must be a valid drafftink plugin exporting
    /// `drafftink_plugin_entry` and returning a valid `Box<dyn Plugin>`.
    pub unsafe fn load(
        &mut self,
        path: &Path,
        ctx: &dyn PluginContext,
    ) -> Result<(), String> {
        log::info!("[plugin] Loading {path:?}");

        // 1. dlopen
        let lib = Library::new(path)
            .map_err(|e| format!("Failed to load library: {e}"))?;

        // 2. Resolve entry symbol
        let entry: Symbol<PluginEntryFn> = lib
            .get(b"drafftink_plugin_entry")
            .map_err(|e| format!("Entry symbol not found: {e}"))?;

        // 3. Call entry → Box<dyn Plugin>
        let raw: *mut dyn Plugin = entry();
        let plugin = unsafe { Box::from_raw(raw) };

        // 4. Read manifest
        let manifest = plugin.manifest().clone();
        log::info!(
            "[plugin] Loaded {} v{} by {}",
            manifest.name, manifest.version, manifest.author,
        );

        // 5. Check permissions
        for perm in &manifest.permissions {
            log::info!("[plugin]   requires: {perm}");
        }

        // 6. on_load
        plugin.on_load(ctx);

        self.loaded.push(LoadedPlugin {
            manifest,
            _lib: lib,
            plugin,
        });

        Ok(())
    }

    // ── Verified Load ────────────────────────────────────────────

    /// Load a plugin with **real** Ed25519 signature verification and audit logging.
    ///
    /// 校验策略（fail-closed，纵深防御）：
    /// 1. 若配置了 `trusted_key`：读取插件二进制与同目录 `.sig` 签名文件，
    ///    在 **dlopen 之前** 用可信公钥校验。校验通过才加载；失败则 **绝不** 加载，
    ///    直接拒绝（防止恶意原生代码 RCE）。
    /// 2. 未配置 `trusted_key` 且处于 `dev_mode`：降级为 dev 信任列表
    ///    （`allowed_devs`，基于 manifest.author）的便利放行——仅限本地开发。
    /// 3. 未配置 `trusted_key` 且非 `dev_mode`（生产）：插件系统不可用，
    ///    拒绝加载任何未签名插件。
    ///
    /// Returns `(result, signature_status)` so the UI can decide whether
    /// to show a warning or block the load.
    ///
    /// # Safety
    ///
    /// This function loads a dynamic library (DLL/so) and calls into
    /// foreign code. The caller must ensure that `path` points to a
    /// trusted plugin file and that the plugin's ABI matches the
    /// expected `drafftink_plugin` trait interface.
    pub unsafe fn load_verified(
        &mut self,
        path: &Path,
        ctx: &dyn PluginContext,
        audit: &mut AuditLogger,
        allowed_devs: &[String],
    ) -> (Result<(), String>, SignatureStatus) {
        let name_guess = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        audit.log_event(&name_guess, "load_attempt", &format!("{path:?}"), "pending", false);

        // 预先读取插件二进制，以便在执行任何插件代码之前完成签名校验。
        let binary = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                let msg = format!("Cannot read plugin file: {e}");
                audit.log_event(&name_guess, "load_attempt", "-", &msg, false);
                return (Err(msg), SignatureStatus::NoSignature);
            }
        };

        // ── 签名校验（生产闸门）────────────────────────────────
        let sig_status = if let Some(pubkey) = self.trusted_key {
            match read_plugin_signature(path) {
                Ok(sig_b64) => {
                    match crate::plugin::signing::verify_signature(&binary, &sig_b64, None, Some(&pubkey))
                    {
                        Ok(crate::plugin::signing::SigStatus::Verified) => SignatureStatus::Verified,
                        Ok(_) => SignatureStatus::Untrusted,
                        Err(e) => {
                            audit.log_event(
                                &name_guess,
                                "verify",
                                "-",
                                &format!("verify error: {e}"),
                                false,
                            );
                            SignatureStatus::Untrusted
                        }
                    }
                }
                Err(e) => {
                    audit.log_event(
                        &name_guess,
                        "verify",
                        "-",
                        &format!("no signature file: {e}"),
                        false,
                    );
                    SignatureStatus::NoSignature
                }
            }
        } else {
            SignatureStatus::NoSignature
        };

        // ── 决策是否加载 ────────────────────────────────────────
        if matches!(sig_status, SignatureStatus::Verified) {
            // 通过真实 Ed25519 校验 → 允许加载（仍执行 dlopen + 入口调用）。
            let (r, _) = self.load_inner(path, ctx, audit);
            return (r, SignatureStatus::Verified);
        }

        if self.trusted_key.is_some() {
            // 生产环境已配置可信密钥，但校验失败 → 拒绝加载（绝不 dlopen）。
            let msg = "Plugin signature verification failed; refusing to load (potential RCE risk)"
                .to_string();
            audit.log_event(&name_guess, "load_blocked", "-", &msg, false);
            return (Err(msg), sig_status);
        }

        if self.dev_mode {
            // 未配置可信密钥 + 开发模式：降级为 dev 信任列表（需加载 DLL 读取 manifest.author）。
            let (r, author) = self.load_inner(path, ctx, audit);
            if let Err(e) = r {
                return (Err(e), sig_status);
            }
            let author = author.unwrap_or_default();
            let dev_known = allowed_devs.iter().any(|d| d == &author);
            if !dev_known {
                let name = self
                    .loaded
                    .last()
                    .map(|l| l.manifest.name.clone())
                    .unwrap_or_default();
                let _ = self.unload(&name);
                let msg = format!("Developer '{author}' not in trusted list");
                audit.log_event(&name, "load_blocked", "-", &msg, false);
                return (Err(msg), SignatureStatus::Untrusted);
            }
            return (Ok(()), SignatureStatus::Verified);
        }

        // 生产环境未配置 trusted_key → 插件系统不可用，拒绝加载（fail closed）。
        let msg = "No trusted_key configured; refusing to load unsigned plugins in production"
            .to_string();
        audit.log_event(&name_guess, "load_blocked", "-", &msg, false);
        (Err(msg), sig_status)
    }

    /// 实际执行 dlopen + 入口调用 + manifest 读取 + on_load + 登记。
    ///
    /// 含 panic 隔离，避免插件入口崩溃拖垮宿主进程。返回加载结果与 manifest.author。
    ///
    /// # Safety
    /// 调用方必须确保 `path` 指向一个合法的 drafftink 插件 cdylib。
    unsafe fn load_inner(
        &mut self,
        path: &Path,
        ctx: &dyn PluginContext,
        audit: &mut AuditLogger,
    ) -> (Result<(), String>, Option<String>) {
        // 1. dlopen
        let lib = match Library::new(path) {
            Ok(l) => l,
            Err(e) => {
                let msg = format!("dlopen failed: {e}");
                audit.log_event("?", "load_attempt", "-", &msg, false);
                return (Err(msg), None);
            }
        };

        // 2. Resolve entry symbol
        let entry: Symbol<PluginEntryFn> = match lib.get(b"drafftink_plugin_entry") {
            Ok(e) => e,
            Err(e) => {
                let msg = format!("Entry symbol missing: {e}");
                audit.log_event("?", "load_attempt", "-", &msg, false);
                return (Err(msg), None);
            }
        };

        // 3. Call entry with panic isolation
        // NOTE: clippy flags `|| entry()` as a redundant closure, but it is NOT:
        // `entry` is a `libloading::Symbol`, which derefs to a fn pointer yet is
        // not itself `FnOnce`, so it cannot be passed directly to
        // `catch_unwind`. The closure is required.
        #[allow(clippy::redundant_closure)]
        let entry_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| entry()));
        let raw = match entry_result {
            Ok(raw) => raw,
            Err(_) => {
                let msg = "Plugin entry panicked".to_string();
                audit.log_event("?", "load_attempt", "-", &msg, false);
                return (Err(msg), None);
            }
        };
        let plugin = Box::from_raw(raw);

        // 4. Read manifest
        let manifest = plugin.manifest().clone();
        let name = manifest.name.clone();
        let author = manifest.author.clone();

        // 5. Permissions check
        for perm in &manifest.permissions {
            log::info!("[plugin:{name}] requires: {perm}");
        }

        // 6. on_load (panic-isolated)
        let on_load_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            plugin.on_load(ctx);
        }));
        if on_load_result.is_err() {
            let msg = "on_load panicked".to_string();
            audit.log_event(&name, "load_blocked", "-", &msg, false);
            return (Err(msg), Some(author));
        }

        // 7. Record
        audit.log_event(&name, "load_success", "-", "ok", true);
        log::info!("[plugin] Loaded {name} v{} by {author}", manifest.version);

        self.loaded.push(LoadedPlugin {
            manifest,
            _lib: lib,
            plugin,
        });

        (Ok(()), Some(author))
    }
    /// # Safety
    ///
    /// 遍历插件目录，对每个 `.dll/.so/.dylib` 调用 [`Self::load_verified`]
    /// （含真实签名校验）。未通过校验的插件直接跳过并记录 `warn!` 日志，
    /// 绝不加载未签名/校验失败的插件。
    pub unsafe fn load_all(
        &mut self,
        ctx: &dyn PluginContext,
        allowed_devs: &[String],
        audit: &mut AuditLogger,
    ) -> Vec<(PathBuf, Result<(), String>)> {
        let paths = self.discover();
        paths
            .into_iter()
            .map(|p| {
                let (r, _sig) = self.load_verified(&p, ctx, audit, allowed_devs);
                if r.is_err() {
                    log::warn!("[plugin] Skipping unverified plugin {p:?}: {r:?}");
                }
                (p, r)
            })
            .collect()
    }

    // ── Unload ───────────────────────────────────────────────────

    pub fn unload(&mut self, name: &str) -> Result<(), String> {
        if let Some(idx) = self.loaded.iter().position(|p| p.manifest.name == name) {
            let removed = self.loaded.swap_remove(idx);
            removed.plugin.on_unload();
            log::info!("[plugin] Unloaded {name}");
        }
        Ok(())
    }

    // ── Query ────────────────────────────────────────────────────

    pub fn loaded_count(&self) -> usize {
        self.loaded.len()
    }

    pub fn loaded_names(&self) -> Vec<&str> {
        self.loaded.iter().map(|p| p.manifest.name.as_str()).collect()
    }

    /// Collect file importers from all loaded plugins.
    pub fn all_importers(&self) -> Vec<(&str, &dyn crate::plugin::api::FileImporter)> {
        self.loaded
            .iter()
            .filter_map(|p| {
                p.plugin
                    .file_importer()
                    .map(|imp| (p.manifest.name.as_str(), imp))
            })
            .collect()
    }

    /// 返回已加载插件的 (名称, 版本) 清单，供上层共享上下文 / UI 展示。
    /// 仅只读访问，不触发任何加载或卸载逻辑。
    pub fn list_loaded(&self) -> Vec<(String, String)> {
        self.loaded
            .iter()
            .map(|l| (l.manifest.name.clone(), l.manifest.version.clone()))
            .collect()
    }
}

impl Drop for PluginManager {
    fn drop(&mut self) {
        eprintln!("[plugin] PluginManager dropping, {} plugins", self.loaded.len());
        for loaded in self.loaded.drain(..) {
            loaded.plugin.on_unload();
            // ★ Key: intentionally leak the DLL to avoid FreeLibrary deadlock on Windows
            std::mem::forget(loaded._lib);
            eprintln!("[plugin] Leaked library: {}", loaded.manifest.name);
        }
    }
}

/// 读取与插件同目录、同名但扩展名为 `.sig` 的签名文件（Base64 编码的 Ed25519 签名）。
fn read_plugin_signature(path: &Path) -> Result<String, String> {
    let sig_path = path.with_extension("sig");
    std::fs::read_to_string(&sig_path).map_err(|e| format!("no .sig at {sig_path:?}: {e}"))
}

// ── DrafftinkPlugin Loader ────────────────────────────────────────

use crate::plugin::drafftink_plugin::{
    DrafftinkPlugin, DrafftinkPluginEntryFn, PluginContext as DynPluginContext,
};

/// A loaded `DrafftinkPlugin` with its library handle and context.
pub struct LoadedDrafftinkPlugin {
    /// The plugin instance (heap-allocated, received from FFI).
    pub plugin: Box<dyn DrafftinkPlugin>,
    /// Plugin context populated during `initialize()`.
    pub context: DynPluginContext,
    /// Kept alive so the library stays loaded; drop → dlclose.
    _lib: Library,
}

/// Loader for `DrafftinkPlugin` cdylibs.
///
/// Scans a directory for `.dll`/`.so`/`.dylib` files, loads each one,
/// resolves the `create_plugin` symbol (generated by `#[export_plugin]`),
/// and calls `initialize()` with a `PluginContext`.
///
/// # Example
///
/// ```ignore
/// let mut loader = DrafftinkPluginLoader::new(PathBuf::from("./modules"));
/// unsafe {
///     loader.load_all().unwrap();
/// }
/// for p in &loader.plugins {
///     println!("Loaded: {} v{}", p.plugin.name(), p.plugin.version());
/// }
/// ```
pub struct DrafftinkPluginLoader {
    modules_dir: PathBuf,
    pub plugins: Vec<LoadedDrafftinkPlugin>,
    /// Trusted Ed25519 verifying key for official plugins.
    trusted_key: Option<[u8; 32]>,
    /// 开发模式：未配置 `trusted_key` 时放行未签名插件（仅本地开发）。
    dev_mode: bool,
}

impl DrafftinkPluginLoader {
    /// Create a new loader that scans `modules_dir`.
    pub fn new(modules_dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&modules_dir);
        Self {
            modules_dir,
            plugins: Vec::new(),
            trusted_key: None,
            dev_mode: false,
        }
    }

    /// Set the trusted public key for official plugin verification.
    pub fn with_trusted_key(mut self, key: [u8; 32]) -> Self {
        self.trusted_key = Some(key);
        self
    }

    /// 启用开发模式：未配置 `trusted_key` 时放行未签名插件。生产环境切勿启用。
    pub fn with_dev_mode(mut self, dev_mode: bool) -> Self {
        self.dev_mode = dev_mode;
        self
    }

    /// Discover all shared libraries in the modules directory.
    pub fn discover(&self) -> Vec<PathBuf> {
        let mut found = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.modules_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    if matches!(ext, "dll" | "so" | "dylib") {
                        found.push(path);
                    }
                }
            }
        }
        found
    }

    /// Load a single plugin from the given path.
    ///
    /// # Safety
    /// The cdylib at `path` must be a valid drafftink plugin exporting
    /// `create_plugin` and returning a valid `Box<dyn DrafftinkPlugin>`.
    pub unsafe fn load(&mut self, path: &Path) -> Result<(), String> {
        log::info!("[dyn-plugin] Loading {path:?}");

        // 1. dlopen
        let lib = Library::new(path)
            .map_err(|e| format!("Failed to load library: {e}"))?;

        // 2. Resolve `create_plugin` symbol
        let entry: Symbol<DrafftinkPluginEntryFn> = lib
            .get(b"create_plugin")
            .map_err(|e| format!("Entry symbol 'create_plugin' not found: {e}"))?;

        // 3. Call entry → Box<dyn DrafftinkPlugin>
        let raw: *mut dyn DrafftinkPlugin = entry();
        let mut plugin = Box::from_raw(raw);

        let name = plugin.name().to_string();
        let version = plugin.version().to_string();
        log::info!("[dyn-plugin] Loaded {name} v{version}");

        // 4. Initialize with context
        let mut ctx = DynPluginContext::new();
        plugin.initialize(&mut ctx);

        log::info!(
            "[dyn-plugin] {} registered {} toolbar actions, {} panels",
            name,
            ctx.toolbar_actions.len(),
            ctx.ui_panels.len(),
        );

        self.plugins.push(LoadedDrafftinkPlugin {
            plugin,
            context: ctx,
            _lib: lib,
        });

        Ok(())
    }

    /// 带真实 Ed25519 签名校验的加载（fail-closed）。
    ///
    /// 策略与 [`PluginManager::load_verified`] 一致：配置了 `trusted_key` 时在
    /// dlopen 前校验 `.sig`；未配置且 `dev_mode` 时放行；否则拒绝。
    ///
    /// # Safety
    /// 调用方必须确保 `path` 指向合法 drafftink 插件 cdylib。
    pub unsafe fn load_verified(
        &mut self,
        path: &Path,
        allowed_devs: &[String],
        audit: &mut AuditLogger,
    ) -> (Result<(), String>, SignatureStatus) {
        let name_guess = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        audit.log_event(
            &name_guess,
            "load_attempt",
            &format!("{path:?}"),
            "pending",
            false,
        );

        let binary = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                let msg = format!("Cannot read plugin file: {e}");
                audit.log_event(&name_guess, "load_attempt", "-", &msg, false);
                return (Err(msg), SignatureStatus::NoSignature);
            }
        };

        let sig_status = if let Some(pubkey) = self.trusted_key {
            match read_plugin_signature(path) {
                Ok(sig_b64) => {
                    match crate::plugin::signing::verify_signature(
                        &binary,
                        &sig_b64,
                        None,
                        Some(&pubkey),
                    ) {
                        Ok(crate::plugin::signing::SigStatus::Verified) => SignatureStatus::Verified,
                        _ => SignatureStatus::Untrusted,
                    }
                }
                Err(_) => SignatureStatus::NoSignature,
            }
        } else {
            SignatureStatus::NoSignature
        };

        if matches!(sig_status, SignatureStatus::Verified) {
            let (r, _) = self.load_inner(path, audit);
            return (r, SignatureStatus::Verified);
        }
        if self.trusted_key.is_some() {
            let msg =
                "Plugin signature verification failed; refusing to load (potential RCE risk)"
                    .to_string();
            audit.log_event(&name_guess, "load_blocked", "-", &msg, false);
            return (Err(msg), sig_status);
        }
        if self.dev_mode {
            // 开发模式便利放行（DrafftinkPlugin 特性不含 author 字段，故不按 dev 名单细粒度校验）。
            let _ = allowed_devs;
            let (r, _) = self.load_inner(path, audit);
            return (r, SignatureStatus::SelfSigned);
        }
        let msg = "No trusted_key configured; refusing to load unsigned plugins in production"
            .to_string();
        audit.log_event(&name_guess, "load_blocked", "-", &msg, false);
        (Err(msg), sig_status)
    }

    /// 执行 dlopen + `create_plugin` 入口 + `initialize` + 登记，含 panic 隔离。
    ///
    /// # Safety
    /// 调用方必须确保 `path` 指向合法 drafftink 插件 cdylib。
    unsafe fn load_inner(
        &mut self,
        path: &Path,
        audit: &mut AuditLogger,
    ) -> (Result<(), String>, Option<String>) {
        log::info!("[dyn-plugin] Loading {path:?}");

        let lib = match Library::new(path) {
            Ok(l) => l,
            Err(e) => {
                let msg = format!("Failed to load library: {e}");
                audit.log_event("?", "load_attempt", "-", &msg, false);
                return (Err(msg), None);
            }
        };

        let entry: Symbol<DrafftinkPluginEntryFn> = match lib.get(b"create_plugin") {
            Ok(e) => e,
            Err(e) => {
                let msg = format!("Entry symbol 'create_plugin' not found: {e}");
                audit.log_event("?", "load_attempt", "-", &msg, false);
                return (Err(msg), None);
            }
        };

        // `|| entry()` is required, not redundant — see note at the other
        // call site. `entry` is a `libloading::Symbol`, not `FnOnce`.
        #[allow(clippy::redundant_closure)]
        let raw: *mut dyn DrafftinkPlugin =
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| entry())) {
                Ok(r) => r,
                Err(_) => {
                    let msg = "Plugin entry panicked".to_string();
                    audit.log_event("?", "load_attempt", "-", &msg, false);
                    return (Err(msg), None);
                }
            };
        let mut plugin = Box::from_raw(raw);

        let name = plugin.name().to_string();
        let version = plugin.version().to_string();

        let mut ctx = DynPluginContext::new();
        let init_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            plugin.initialize(&mut ctx);
        }));
        if init_result.is_err() {
            let msg = "plugin.initialize panicked".to_string();
            audit.log_event(&name, "load_blocked", "-", &msg, false);
            return (Err(msg), Some(name));
        }

        log::info!("[dyn-plugin] Loaded {name} v{version}");
        log::info!(
            "[dyn-plugin] {name} registered {} toolbar actions, {} panels",
            ctx.toolbar_actions.len(),
            ctx.ui_panels.len(),
        );

        self.plugins.push(LoadedDrafftinkPlugin {
            plugin,
            context: ctx,
            _lib: lib,
        });

        (Ok(()), Some(name))
    }

    /// Load all plugins discovered in the modules directory.
    ///
    /// # Safety
    /// 遍历插件目录，对每个 `.dll/.so/.dylib` 调用 [`Self::load_verified`]
    /// （含真实签名校验）。未通过校验的插件直接跳过并记录 `warn!` 日志。
    pub unsafe fn load_all(
        &mut self,
        allowed_devs: &[String],
        audit: &mut AuditLogger,
    ) -> Vec<(PathBuf, Result<(), String>)> {
        let paths = self.discover();
        paths
            .into_iter()
            .map(|p| {
                let (result, _sig) = self.load_verified(&p, allowed_devs, audit);
                if result.is_err() {
                    log::warn!("[dyn-plugin] Skipping unverified plugin {p:?}: {result:?}");
                }
                (p, result)
            })
            .collect()
    }

    /// Unload a plugin by name (calls `shutdown()`, then drops the library).
    pub fn unload(&mut self, name: &str) -> Result<(), String> {
        if let Some(idx) = self.plugins.iter().position(|p| p.plugin.name() == name) {
            let mut removed = self.plugins.swap_remove(idx);
            removed.plugin.shutdown();
            log::info!("[dyn-plugin] Unloaded {name}");
            // Intentionally leak the library to avoid Windows FreeLibrary deadlock
            std::mem::forget(removed._lib);
        }
        Ok(())
    }

    /// Number of loaded plugins.
    pub fn loaded_count(&self) -> usize {
        self.plugins.len()
    }

    /// Names of all loaded plugins.
    pub fn loaded_names(&self) -> Vec<&str> {
        self.plugins.iter().map(|p| p.plugin.name()).collect()
    }

    /// Collect all toolbar actions from loaded plugins.
    pub fn all_toolbar_actions(&self) -> Vec<&crate::plugin::ToolbarAction> {
        self.plugins
            .iter()
            .flat_map(|p| p.context.toolbar_actions.iter())
            .collect()
    }
}

impl Drop for DrafftinkPluginLoader {
    fn drop(&mut self) {
        eprintln!(
            "[dyn-plugin] DrafftinkPluginLoader dropping, {} plugins",
            self.plugins.len()
        );
        for mut loaded in self.plugins.drain(..) {
            loaded.plugin.shutdown();
            // Intentionally leak the DLL to avoid FreeLibrary deadlock on Windows
            std::mem::forget(loaded._lib);
            eprintln!("[dyn-plugin] Leaked library: {}", loaded.plugin.name());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "drafftink_loader_test_{}_{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn read_plugin_signature_reads_dot_sig() {
        let dir = temp_dir();
        let dll = dir.join("plugin.dll");
        let sig = dir.join("plugin.sig");
        std::fs::write(&dll, b"fake binary").unwrap();
        std::fs::write(&sig, "BASE64SIGNATURE").unwrap();
        let got = read_plugin_signature(&dll).unwrap();
        assert_eq!(got, "BASE64SIGNATURE");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_plugin_signature_missing_returns_err() {
        let dir = temp_dir();
        let dll = dir.join("plugin.dll");
        std::fs::write(&dll, b"fake binary").unwrap();
        assert!(read_plugin_signature(&dll).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_verified_refuses_without_trusted_key_and_not_dev() {
        // 即便磁盘上存在“插件”文件，未配置 trusted_key 且非 dev_mode 也必须拒绝加载。
        let dir = temp_dir();
        let dll = dir.join("plugin.dll");
        std::fs::write(&dll, b"fake binary").unwrap();

        let mut pm = PluginManager::new(dir.clone());
        let mut audit = AuditLogger::new(&dir).unwrap();
        let ctx = crate::plugin::api::DummyContext;
        let (res, _sig) = unsafe { pm.load_verified(&dll, &ctx, &mut audit, &[]) };
        assert!(
            res.is_err(),
            "未配置 trusted_key 且非 dev_mode 必须拒绝加载（fail-closed）"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
