//! File-based logging configuration.
//!
//! This module provides optional file-based logging using tracing-appender.
//! When enabled via the `VK_FILE_LOGGING` environment variable, logs are written
//! to rotating daily log files in addition to console output.
//!
//! # Configuration
//!
//! - `VK_FILE_LOGGING`: Set to "true" or "1" to enable file logging
//! - `VK_LOG_DIR`: Override default log directory (defaults to `{asset_dir}/logs`)
//! - `VK_LOG_MAX_FILES`: Number of daily log files to retain (default: 7)
//!
//! # Log Format
//!
//! Log files use JSON format for easier parsing and analysis:
//! ```json
//! {"timestamp":"2025-12-26T10:30:00Z","level":"INFO","target":"server","message":"..."}
//! ```

use std::path::PathBuf;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};
use utils::assets::asset_dir;

/// Configuration for file logging.
#[derive(Debug, Clone)]
pub struct FileLoggingConfig {
    /// Whether file logging is enabled.
    pub enabled: bool,
    /// Directory to write log files to.
    pub log_dir: PathBuf,
    /// Number of daily log files to retain.
    pub max_files: usize,
}

impl Default for FileLoggingConfig {
    fn default() -> Self {
        let enabled = std::env::var("VK_FILE_LOGGING")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        let log_dir = std::env::var("VK_LOG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| asset_dir().join("logs"));

        let max_files = std::env::var("VK_LOG_MAX_FILES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(7);

        Self {
            enabled,
            log_dir,
            max_files,
        }
    }
}

/// Initialize the logging system with optional file output.
///
/// Returns a guard that must be held for the lifetime of the application
/// to ensure all logs are flushed. If file logging is not enabled, returns None.
///
/// # Arguments
/// * `log_level` - The base log level (e.g., "info", "debug")
///
/// # Example
/// ```ignore
/// let _guard = init_logging("info");
/// // ... application runs ...
/// // guard is dropped on shutdown, flushing remaining logs
/// ```
pub fn init_logging(log_level: &str) -> Option<WorkerGuard> {
    let config = FileLoggingConfig::default();

    // Build the filter string for our crates at the specified level
    let filter_string = format!(
        "warn,server={level},services={level},db={level},executors={level},deployment={level},local_deployment={level},utils={level}",
        level = log_level
    );
    let env_filter = EnvFilter::try_new(&filter_string).expect("Failed to create tracing filter");

    // Create the console layer (always enabled)
    let console_layer = tracing_subscriber::fmt::layer()
        .with_filter(env_filter.clone());

    if config.enabled {
        // Create log directory if it doesn't exist
        if let Err(e) = std::fs::create_dir_all(&config.log_dir) {
            eprintln!("Failed to create log directory {:?}: {}", config.log_dir, e);
            // Fall back to console-only logging
            tracing_subscriber::registry()
                .with(console_layer)
                .init();
            return None;
        }

        // Set up daily rotating file appender
        let file_appender = tracing_appender::rolling::daily(&config.log_dir, "vibe-kanban.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        // Create file layer with JSON format for easier parsing
        let file_filter = EnvFilter::try_new(&filter_string).expect("Failed to create file filter");
        let file_layer = tracing_subscriber::fmt::layer()
            .json()
            .with_writer(non_blocking)
            .with_filter(file_filter);

        tracing_subscriber::registry()
            .with(console_layer)
            .with(file_layer)
            .init();

        tracing::info!(
            log_dir = ?config.log_dir,
            max_files = config.max_files,
            "File logging enabled"
        );

        // Spawn background task to clean up old log files
        let log_dir = config.log_dir.clone();
        let max_files = config.max_files;
        std::thread::spawn(move || {
            cleanup_old_logs(&log_dir, max_files);
        });

        Some(guard)
    } else {
        // Console-only logging
        tracing_subscriber::registry()
            .with(console_layer)
            .init();
        None
    }
}

/// Clean up old log files, keeping only the most recent `max_files`.
fn cleanup_old_logs(log_dir: &PathBuf, max_files: usize) {
    let entries = match std::fs::read_dir(log_dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    let mut log_files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("vibe-kanban.log"))
                .unwrap_or(false)
        })
        .filter_map(|e| {
            e.metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| (e.path(), t))
        })
        .collect();

    // Sort by modification time, newest first
    log_files.sort_by(|a, b| b.1.cmp(&a.1));

    // Remove files beyond max_files
    for (path, _) in log_files.into_iter().skip(max_files) {
        if let Err(e) = std::fs::remove_file(&path) {
            tracing::warn!("Failed to remove old log file {:?}: {}", path, e);
        } else {
            tracing::debug!("Removed old log file: {:?}", path);
        }
    }
}
