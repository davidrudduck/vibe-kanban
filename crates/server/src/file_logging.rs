//! File-based logging configuration.
//!
//! Optional file output using tracing-appender. When enabled via `VK_FILE_LOGGING`,
//! logs are written to daily-rotating JSON files in addition to console output.
//!
//! # Environment variables
//!
//! - `VK_FILE_LOGGING` — set to `"true"` or `"1"` to enable (default: off)
//! - `VK_LOG_DIR` — override log directory (default: `{asset_dir}/logs`)
//! - `VK_LOG_MAX_FILES` — daily files to retain (default: `7`)

use std::path::{Path, PathBuf};

use tracing_appender::non_blocking::WorkerGuard;

pub struct FileLoggingConfig {
    pub enabled: bool,
    pub log_dir: PathBuf,
    pub max_files: usize,
}

impl FileLoggingConfig {
    pub fn from_env(asset_dir: PathBuf) -> Self {
        let enabled = std::env::var("VK_FILE_LOGGING")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        let log_dir = std::env::var("VK_LOG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| asset_dir.join("logs"));

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

pub fn init_logging(filter_string: &str) -> Option<WorkerGuard> {
    todo!()
}

fn cleanup_old_logs(log_dir: &Path, max_files: usize) {
    let entries = match std::fs::read_dir(log_dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("Failed to read log directory {:?}: {}", log_dir, e);
            return;
        }
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

    // Newest first
    log_files.sort_by(|a, b| b.1.cmp(&a.1));

    for (path, _) in log_files.into_iter().skip(max_files) {
        if let Err(e) = std::fs::remove_file(&path) {
            tracing::warn!("Failed to remove old log file {:?}: {}", path, e);
        } else {
            tracing::debug!("Removed old log file: {:?}", path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "vk-log-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn defaults_to_disabled() {
        unsafe {
            std::env::remove_var("VK_FILE_LOGGING");
            std::env::remove_var("VK_LOG_DIR");
            std::env::remove_var("VK_LOG_MAX_FILES");
        }

        let asset = temp_dir();
        let config = FileLoggingConfig::from_env(asset.clone());

        assert!(!config.enabled);
        assert_eq!(config.log_dir, asset.join("logs"));
        assert_eq!(config.max_files, 7);
    }

    #[test]
    fn enabled_by_true_string() {
        unsafe {
            std::env::set_var("VK_FILE_LOGGING", "true");
        }
        let config = FileLoggingConfig::from_env(temp_dir());
        assert!(config.enabled);
        unsafe {
            std::env::remove_var("VK_FILE_LOGGING");
        }
    }

    #[test]
    fn enabled_by_one_string() {
        unsafe {
            std::env::set_var("VK_FILE_LOGGING", "1");
        }
        let config = FileLoggingConfig::from_env(temp_dir());
        assert!(config.enabled);
        unsafe {
            std::env::remove_var("VK_FILE_LOGGING");
        }
    }

    #[test]
    fn not_enabled_by_other_values() {
        for val in &["yes", "TRUE", "on", "false", "0"] {
            unsafe {
                std::env::set_var("VK_FILE_LOGGING", val);
            }
            let config = FileLoggingConfig::from_env(temp_dir());
            assert!(
                !config.enabled,
                "expected disabled for VK_FILE_LOGGING={val}"
            );
        }
        unsafe {
            std::env::remove_var("VK_FILE_LOGGING");
        }
    }

    #[test]
    fn log_dir_overridden_by_env() {
        let custom = temp_dir();
        unsafe {
            std::env::set_var("VK_LOG_DIR", custom.to_str().unwrap());
        }
        let config = FileLoggingConfig::from_env(temp_dir());
        assert_eq!(config.log_dir, custom);
        unsafe {
            std::env::remove_var("VK_LOG_DIR");
        }
    }

    #[test]
    fn max_files_overridden_by_env() {
        unsafe {
            std::env::set_var("VK_LOG_MAX_FILES", "14");
        }
        let config = FileLoggingConfig::from_env(temp_dir());
        assert_eq!(config.max_files, 14);
        unsafe {
            std::env::remove_var("VK_LOG_MAX_FILES");
        }
    }

    #[test]
    fn invalid_max_files_falls_back_to_default() {
        unsafe {
            std::env::set_var("VK_LOG_MAX_FILES", "not-a-number");
        }
        let config = FileLoggingConfig::from_env(temp_dir());
        assert_eq!(config.max_files, 7);
        unsafe {
            std::env::remove_var("VK_LOG_MAX_FILES");
        }
    }

    #[test]
    fn cleanup_keeps_newest_n_files() {
        let dir = temp_dir();

        for i in 0..10u8 {
            let path = dir.join(format!("vibe-kanban.log.2025-01-{:02}", i + 1));
            fs::write(&path, b"log").unwrap();
        }

        cleanup_old_logs(&dir, 3);

        let remaining: Vec<_> = fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).collect();

        assert_eq!(remaining.len(), 3, "should retain exactly 3 files");
    }

    #[test]
    fn cleanup_is_noop_when_under_limit() {
        let dir = temp_dir();
        for i in 0..3u8 {
            fs::write(
                dir.join(format!("vibe-kanban.log.2025-01-{:02}", i + 1)),
                b"x",
            )
            .unwrap();
        }

        cleanup_old_logs(&dir, 7);

        let remaining: Vec<_> = fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).collect();
        assert_eq!(remaining.len(), 3);
    }

    #[test]
    fn cleanup_ignores_non_log_files() {
        let dir = temp_dir();
        fs::write(dir.join("vibe-kanban.log.2025-01-01"), b"log").unwrap();
        fs::write(dir.join("unrelated.txt"), b"other").unwrap();

        cleanup_old_logs(&dir, 0); // keep 0 log files

        assert!(dir.join("unrelated.txt").exists());
    }
}
