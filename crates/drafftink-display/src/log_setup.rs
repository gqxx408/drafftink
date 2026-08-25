//! Logging setup — writes to `logs/display_YYYY-MM-DD.log` in the runtime directory.

use std::fs;
use std::path::PathBuf;

use log::LevelFilter;
use simplelog::{CombinedLogger, ConfigBuilder, WriteLogger};

/// Initialise file-based logging: `./logs/display_YYYY-MM-DD.log`.
pub fn init_logger() -> PathBuf {
    let log_dir = std::env::current_dir().unwrap_or_default().join("logs");
    let _ = fs::create_dir_all(&log_dir);

    let today = chrono::Utc::now().format("%Y-%m-%d");
    let log_path = log_dir.join(format!("display_{}.log", today));

    let file = match fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        Ok(f) => f,
        Err(_) => {
            // Fallback: terminal-only via env_logger
            let _ =
                env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
                    .try_init();
            return log_path;
        }
    };

    let config = ConfigBuilder::new().set_time_format_rfc3339().build();

    // File logger — all log::info!/warn!/error! go here
    let _ = CombinedLogger::init(vec![WriteLogger::new(LevelFilter::Info, config, file)]);

    log::info!("Log started → {:?}", log_path);
    log_path
}
