//! Permission sandbox — checks whether a plugin is allowed to perform
//! an operation, and isolates crashes so they don't bring down the host.

use crate::plugin::api::Permission;
use std::collections::HashSet;

/// Tracks which permissions have been granted to each plugin.
#[derive(Default)]
pub struct PermissionStore {
    /// plugin name → set of granted permissions
    grants: Vec<(String, HashSet<Permission>)>,
}

impl PermissionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn grant(&mut self, plugin_name: &str, perms: &[Permission]) {
        self.grants.retain(|(n, _)| n != plugin_name);
        self.grants
            .push((plugin_name.to_string(), perms.iter().cloned().collect()));
    }

    pub fn revoke(&mut self, plugin_name: &str) {
        self.grants.retain(|(n, _)| n != plugin_name);
    }

    pub fn is_granted(&self, plugin_name: &str, perm: &Permission) -> bool {
        self.grants
            .iter()
            .any(|(n, s)| n == plugin_name && s.contains(perm))
    }
}

// ── Panic isolation ──────────────────────────────────────────────

/// Run a closure inside a catch_unwind boundary to isolate plugin panics.
///
/// Returns `Ok(value)` on success, `Err(panic_message)` if the plugin panicked.
pub fn isolate_plugin_call<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce() -> R + std::panic::UnwindSafe,
{
    std::panic::catch_unwind(f).map_err(|payload| {
        let msg = if let Some(s) = payload.downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = payload.downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic".to_string()
        };
        eprintln!("[plugin] Panic caught: {msg}");
        msg
    })
}

/// Safe wrapper with plugin name and operation context for audit logging.
pub fn safe_call<F, R>(plugin_name: &str, operation: &str, f: F) -> Result<R, String>
where
    F: FnOnce() -> R + std::panic::UnwindSafe,
{
    let result = isolate_plugin_call(f);
    if let Err(ref msg) = result {
        eprintln!("[plugin:{plugin_name}] PANIC during '{operation}' — isolated: {msg}");
    }
    result
}
