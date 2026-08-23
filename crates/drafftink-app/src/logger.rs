//! File-based logger with per-module verbosity.
//!
//! - `enbx_importer` always logs at **Debug** level (no env var needed).
//! - Everything else defaults to **Info**, or **Debug** if `RUST_LOG=debug`.
//! - Writes to `%TEMP%/seewo.log` + stderr, auto-rotates at 5 MiB.

use log::{LevelFilter, Log, Metadata, Record};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

const MAX_LOG_SIZE: u64 = 5 * 1024 * 1024;

struct FileLogger {
    file: Mutex<Option<File>>,
    default_level: LevelFilter,
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.effective_level(metadata) != LevelFilter::Off
    }

    fn log(&self, record: &Record) {
        let eff = self.effective_level(record.metadata());
        if record.metadata().level() > eff {
            return;
        }
        let ts = now();
        let module = record.module_path().unwrap_or(record.target());
        let line = format!(
            "[{}] {:5} {module} — {}\n",
            ts,
            record.level(),
            record.args(),
        );
        eprint!("{line}");
        if let Ok(mut g) = self.file.lock() {
            if let Some(ref mut f) = *g {
                let _ = f.write_all(line.as_bytes());
                let _ = f.flush();
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut g) = self.file.lock() {
            if let Some(ref mut f) = *g {
                let _ = f.flush();
            }
        }
    }
}

impl FileLogger {
    /// Effective max level for a given log target.
    fn effective_level(&self, meta: &Metadata) -> LevelFilter {
        let t = meta.target();
        // Always debug for enbx_importer, plugin, and seewo_class
        if t.contains("enbx_importer") || t.contains("seewo_class") {
            LevelFilter::Debug
        } else {
            self.default_level
        }
    }
}

// ----- public ---------------------------------------------------------

pub fn init(global_level: LevelFilter) -> Box<dyn std::any::Any> {
    let path = log_path();
    if let Some(p) = path.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > MAX_LOG_SIZE {
            let _ = std::fs::rename(&path, path.with_extension("log.old"));
        }
    }
    let file = OpenOptions::new().create(true).append(true).open(&path).ok();
    let path_s = path.display().to_string();
    let logger = FileLogger {
        file: Mutex::new(file),
        default_level: global_level,
    };
    if log::set_boxed_logger(Box::new(logger)).is_err() {
        return Box::new(());
    }
    log::set_max_level(LevelFilter::Debug); // allow debug globally
    log::info!("=== Session start, log: {path_s} ===");
    Box::new(LoggerGuard)
}

struct LoggerGuard;
impl Drop for LoggerGuard {
    fn drop(&mut self) { log::logger().flush(); }
}

fn log_path() -> PathBuf {
    if cfg!(target_os = "windows") {
        if let Ok(dir) = std::env::var("LOCALAPPDATA") {
            return PathBuf::from(dir).join("SeewoClass").join("seewo.log");
        }
    }
    std::env::temp_dir().join("seewo.log")
}

// ----- timestamp ------------------------------------------------------

fn now() -> String {
    use std::time::SystemTime;
    let dur = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    let secs = dur.as_secs();
    let days = (secs / 86400) as i64;
    let tod = (secs % 86400) as u32;
    let h = tod / 3600;
    let m = (tod % 3600) / 60;
    let s = tod % 60;
    let (y, mo, d) = days_to_date(days);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}:{s:02}")
}

fn days_to_date(mut days: i64) -> (i64, u32, u32) {
    days += 719468;
    let era = if days >= 0 { days / 146097 } else { (days - 146096) / 146097 };
    let doe = (days - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}
