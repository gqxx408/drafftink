//! Plugin audit log — writes every plugin operation to a JSONL file.
//!
//! Format: one JSON object per line, append-only.
//! Fields: ts, plugin, action, params, result, user_confirmed.

use std::fs::OpenOptions;
use std::io::{BufWriter, Write};

/// Appends audit entries to `plugin_audit_YYYYMMDD.jsonl`.
pub struct AuditLogger {
    writer: BufWriter<std::fs::File>,
}

impl AuditLogger {
    /// Create or open today's audit log in `log_dir`.
    pub fn new(log_dir: &std::path::Path) -> Result<Self, String> {
        std::fs::create_dir_all(log_dir).map_err(|e| e.to_string())?;

        let today = chrono::Utc::now().format("%Y%m%d");
        let log_path = log_dir.join(format!("plugin_audit_{today}.jsonl"));
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| format!("Cannot open audit log {log_path:?}: {e}"))?;

        Ok(Self {
            writer: BufWriter::new(file),
        })
    }

    /// Record a plugin action.
    pub fn log_event(
        &mut self,
        plugin_name: &str,
        action: &str,
        params: &str,
        result: &str,
        user_confirmed: bool,
    ) {
        let entry = serde_json::json!({
            "ts": chrono::Utc::now().to_rfc3339(),
            "plugin": plugin_name,
            "action": action,
            "params": params,
            "result": result,
            "user_confirmed": user_confirmed,
        });

        if let Ok(line) = serde_json::to_string(&entry) {
            let _ = writeln!(self.writer, "{line}");
            let _ = self.writer.flush();
        }
    }
}

/// No-op logger for when audit is disabled.
pub struct NullAudit;

impl NullAudit {
    #[allow(unused)]
    pub fn log_event(
        &mut self,
        _plugin: &str,
        _action: &str,
        _params: &str,
        _result: &str,
        _ok: bool,
    ) {
    }
}
