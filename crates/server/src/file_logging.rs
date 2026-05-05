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
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};
use utils::{assets::asset_dir, sentry::sentry_layer};

pub struct FileLoggingConfig {
    pub enabled: bool,
    pub log_dir: PathBuf,
    pub max_files: usize,
    /// Maximum number of log lines buffered before the non-blocking writer
    /// either drops (lossy=true) or blocks (lossy=false). Default: 128_000.
    pub buffer_lines: usize,
    /// When `true` (default), excess log lines are dropped under load rather
    /// than blocking the application. Set `VK_LOG_LOSSY=false` (or `0`) to
    /// block instead (useful for debugging; adds latency under log bursts).
    ///
    /// Note: only the exact values `"false"` and `"0"` disable lossy mode;
    /// all other values (including unrecognised strings) keep lossy enabled.
    pub lossy: bool,
}

impl FileLoggingConfig {
    pub fn from_env(asset_dir: PathBuf) -> Self {
        let enabled = std::env::var("VK_FILE_LOGGING")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        let log_dir = std::env::var("VK_LOG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| asset_dir.join("logs"));

        let raw_max = std::env::var("VK_LOG_MAX_FILES")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(7);
        let max_files = if raw_max == 0 {
            eprintln!("VK_LOG_MAX_FILES=0 is invalid (minimum is 1); using 1");
            1
        } else {
            raw_max
        };

        let raw_buffer = std::env::var("VK_LOG_BUFFER_LINES")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(128_000);
        let buffer_lines = if raw_buffer == 0 {
            eprintln!("VK_LOG_BUFFER_LINES=0 is invalid (minimum is 1); using 1");
            1
        } else {
            raw_buffer
        };

        let lossy = std::env::var("VK_LOG_LOSSY")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);

        Self {
            enabled,
            log_dir,
            max_files,
            buffer_lines,
            lossy,
        }
    }
}

/// Build the tracing filter string from the `RUST_LOG` environment variable.
///
/// If `RUST_LOG` contains `=` or `,` it is treated as a full directive string
/// and passed through verbatim (e.g. `"warn,hyper=error"`). Otherwise it is
/// treated as a plain level name and interpolated into per-crate directives.
///
/// This prevents a panic when `RUST_LOG` holds a full directive and it is
/// naively used as a `{level}` placeholder.
pub fn build_filter_string() -> String {
    let rust_log = std::env::var("RUST_LOG").unwrap_or_default();

    if rust_log.contains('=') || rust_log.contains(',') {
        // Full directive — pass through as-is
        rust_log
    } else {
        let level = if rust_log.is_empty() {
            "info".to_string()
        } else {
            rust_log
        };
        format!(
            "warn,server={level},services={level},db={level},executors={level},\
deployment={level},local_deployment={level},utils={level},embedded_ssh={level},\
desktop_bridge={level},relay_hosts={level},relay_client={level},\
relay_webrtc={level},codex_core=off"
        )
    }
}

/// Initialise the tracing subscriber with optional file output.
///
/// Returns a [`WorkerGuard`] when file logging is enabled — **hold it for the
/// entire lifetime of the process** so buffered log lines are flushed on exit.
/// Dropping it early will stop file logging silently.
///
/// `filter_string` is a `tracing-subscriber` filter directive, e.g.
/// `"warn,server=info,services=info"`.
///
/// # Panics
/// Panics if `filter_string` is not a valid `EnvFilter` directive string.
pub fn init_logging(filter_string: &str) -> Option<WorkerGuard> {
    let config = FileLoggingConfig::from_env(asset_dir());

    let env_filter = EnvFilter::try_new(filter_string).expect("Failed to create tracing filter");
    let console_layer = tracing_subscriber::fmt::layer().with_filter(env_filter);

    if config.enabled {
        if let Err(e) = std::fs::create_dir_all(&config.log_dir) {
            eprintln!(
                "Failed to create log directory {:?}: {} — falling back to console-only logging",
                config.log_dir, e
            );
            if let Err(e) = tracing_subscriber::registry()
                .with(console_layer)
                .with(sentry_layer())
                .try_init()
            {
                eprintln!("Tracing subscriber already initialised: {e}");
            }
            return None;
        }

        let file_appender = tracing_appender::rolling::daily(&config.log_dir, "vibe-kanban.log");
        let (non_blocking, guard) = tracing_appender::non_blocking::NonBlockingBuilder::default()
            .buffered_lines_limit(config.buffer_lines)
            .lossy(config.lossy)
            .finish(file_appender);

        let file_filter =
            EnvFilter::try_new(filter_string).expect("Failed to create file tracing filter");
        let file_layer = tracing_subscriber::fmt::layer()
            .json()
            .with_writer(non_blocking)
            .with_filter(file_filter);

        if let Err(e) = tracing_subscriber::registry()
            .with(console_layer)
            .with(file_layer)
            .with(sentry_layer())
            .try_init()
        {
            eprintln!("Tracing subscriber already initialised: {e}");
            // File layer was never installed — drop the guard to release the
            // background writer thread, and signal no file logging to the caller.
            drop(guard);
            return None;
        }

        tracing::info!(
            log_dir = ?config.log_dir,
            max_files = config.max_files,
            buffer_lines = config.buffer_lines,
            lossy = config.lossy,
            "File logging enabled"
        );

        let log_dir = config.log_dir.clone();
        let max_files = config.max_files;
        // Fire-and-forget: cleanup is fast and idempotent. If the process exits
        // before this thread completes, at most a partial deletion occurs — not
        // data-corrupting.
        std::thread::spawn(move || cleanup_old_logs(&log_dir, max_files));

        Some(guard)
    } else {
        if let Err(e) = tracing_subscriber::registry()
            .with(console_layer)
            .with(sentry_layer())
            .try_init()
        {
            eprintln!("Tracing subscriber already initialised: {e}");
        }
        None
    }
}

/// Returns `true` only for filenames matching `vibe-kanban.log.YYYY-MM-DD`.
///
/// Strict: the date part must be exactly 10 characters, ASCII digits in the
/// right positions, dashes at positions 4 and 7, month in 01–12, and day in
/// 01–31 (calendar accuracy is not required; structural validity is enough to
/// distinguish date-suffix files from `.bak`, `.old`, etc.).
pub(crate) fn is_log_date_suffix(name: &str) -> bool {
    const PREFIX: &str = "vibe-kanban.log.";
    let Some(suffix) = name.strip_prefix(PREFIX) else {
        return false;
    };
    if suffix.len() != 10 {
        return false;
    }
    let b = suffix.as_bytes();
    // YYYY-MM-DD: digits at 0-3, dash at 4, digits at 5-6, dash at 7, digits at 8-9
    if !(b[0].is_ascii_digit()
        && b[1].is_ascii_digit()
        && b[2].is_ascii_digit()
        && b[3].is_ascii_digit()
        && b[4] == b'-'
        && b[5].is_ascii_digit()
        && b[6].is_ascii_digit()
        && b[7] == b'-'
        && b[8].is_ascii_digit()
        && b[9].is_ascii_digit())
    {
        return false;
    }
    // Range-check month (01-12) and day (01-31).
    let month = (b[5] - b'0') * 10 + (b[6] - b'0');
    let day = (b[8] - b'0') * 10 + (b[9] - b'0');
    (1..=12).contains(&month) && (1..=31).contains(&day)
}

pub(crate) fn cleanup_old_logs(log_dir: &Path, max_files: usize) {
    let entries = match std::fs::read_dir(log_dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("Failed to read log directory {:?}: {}", log_dir, e);
            return;
        }
    };

    // Collect only files matching `vibe-kanban.log.YYYY-MM-DD`.
    // Extract the date suffix for sorting — YYYY-MM-DD is lexicographically
    // monotonic, so string sort == chronological sort with no mtime dependency.
    let mut log_files: Vec<(std::path::PathBuf, String)> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_owned();
            if is_log_date_suffix(&name) {
                let date = name["vibe-kanban.log.".len()..].to_owned();
                Some((e.path(), date))
            } else {
                None
            }
        })
        .collect();

    // Newest date first (reverse lexicographic).
    log_files.sort_by(|a, b| b.1.cmp(&a.1));

    // Defensive floor: from_env enforces ≥ 1, but protect direct callers too.
    let keep = max_files.max(1);
    for (path, _) in log_files.into_iter().skip(keep) {
        if let Err(e) = std::fs::remove_file(&path) {
            tracing::warn!("Failed to remove old log file {:?}: {}", path, e);
        } else {
            tracing::debug!("Removed old log file: {:?}", path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::Mutex};

    use super::*;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

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
        let _lock = ENV_LOCK.lock().unwrap();
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
        let _lock = ENV_LOCK.lock().unwrap();
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
        let _lock = ENV_LOCK.lock().unwrap();
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
        let _lock = ENV_LOCK.lock().unwrap();
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
        let _lock = ENV_LOCK.lock().unwrap();
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
        let _lock = ENV_LOCK.lock().unwrap();
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
        let _lock = ENV_LOCK.lock().unwrap();
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
    fn cleanup_keeps_newest_n_by_date() {
        let dir = temp_dir();
        // Create 10 files with explicit dates — newest 3 must survive
        for i in 1..=10u32 {
            let path = dir.join(format!("vibe-kanban.log.2025-01-{:02}", i));
            fs::write(&path, b"log").unwrap();
        }

        cleanup_old_logs(&dir, 3);

        let remaining: std::collections::HashSet<String> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();

        assert_eq!(remaining.len(), 3, "expected exactly 3 files; got {:?}", remaining);
        assert!(remaining.contains("vibe-kanban.log.2025-01-08"), "day 08 missing");
        assert!(remaining.contains("vibe-kanban.log.2025-01-09"), "day 09 missing");
        assert!(remaining.contains("vibe-kanban.log.2025-01-10"), "day 10 missing");
    }

    #[test]
    fn cleanup_rejects_non_date_suffix_files() {
        let dir = temp_dir();
        // These must NEVER be deleted — they don't match the date suffix pattern
        for name in &[
            "vibe-kanban.log",           // no date suffix
            "vibe-kanban.log.bak",       // backup extension
            "vibe-kanban.log.old",       // old extension
            "vibe-kanban.log.2025-1-1",  // wrong date format (not zero-padded)
            "vibe-kanban.log.2025-13-01",// invalid month
        ] {
            fs::write(dir.join(name), b"x").unwrap();
        }
        // One real log file — with max_files=1, only the one date-file is kept
        fs::write(dir.join("vibe-kanban.log.2025-01-01"), b"log").unwrap();

        cleanup_old_logs(&dir, 1); // keep 1 log file

        // The one real log file must survive
        assert!(
            dir.join("vibe-kanban.log.2025-01-01").exists(),
            "real log file was deleted"
        );
        // All non-date files must still exist
        for name in &[
            "vibe-kanban.log",
            "vibe-kanban.log.bak",
            "vibe-kanban.log.old",
            "vibe-kanban.log.2025-1-1",
            "vibe-kanban.log.2025-13-01",
        ] {
            assert!(dir.join(name).exists(), "{name} was deleted but should not be");
        }
    }

    #[test]
    fn max_files_zero_is_clamped_to_one() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("VK_LOG_MAX_FILES", "0");
        }
        let config = FileLoggingConfig::from_env(temp_dir());
        assert_eq!(config.max_files, 1, "max_files=0 must be clamped to 1");
        unsafe {
            std::env::remove_var("VK_LOG_MAX_FILES");
        }
    }

    #[test]
    fn is_log_date_suffix_accepts_valid_names() {
        assert!(is_log_date_suffix("vibe-kanban.log.2025-01-15"));
        assert!(is_log_date_suffix("vibe-kanban.log.2099-12-31"));
    }

    #[test]
    fn is_log_date_suffix_rejects_invalid_names() {
        assert!(!is_log_date_suffix("vibe-kanban.log"));
        assert!(!is_log_date_suffix("vibe-kanban.log.bak"));
        assert!(!is_log_date_suffix("vibe-kanban.log.2025-1-1"));
        assert!(!is_log_date_suffix("vibe-kanban.log.2025-13-01"));
        assert!(!is_log_date_suffix("vibe-kanban.log.abcd-ef-gh"));
        assert!(!is_log_date_suffix("other.log.2025-01-01"));
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
    fn build_filter_string_with_plain_level_interpolates() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::remove_var("RUST_LOG");
        }
        let s = build_filter_string();
        assert!(s.contains("server=info"), "got: {s}");
        assert!(s.contains("services=info"), "got: {s}");
        assert!(s.contains("codex_core=off"), "got: {s}");
    }

    #[test]
    fn build_filter_string_with_full_directive_passes_through() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("RUST_LOG", "warn,hyper=error");
        }
        let s = build_filter_string();
        assert_eq!(s, "warn,hyper=error", "got: {s}");
        unsafe {
            std::env::remove_var("RUST_LOG");
        }
    }

    #[test]
    fn build_filter_string_with_plain_warn_level() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("RUST_LOG", "warn");
        }
        let s = build_filter_string();
        assert!(s.contains("server=warn"), "got: {s}");
        assert!(!s.contains("server=warn,warn"), "double-interpolated: {s}");
        unsafe {
            std::env::remove_var("RUST_LOG");
        }
    }

    #[test]
    fn build_filter_string_with_equals_directive_passes_through() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("RUST_LOG", "server=debug");
        }
        let s = build_filter_string();
        assert_eq!(s, "server=debug", "got: {s}");
        unsafe {
            std::env::remove_var("RUST_LOG");
        }
    }

    #[test]
    fn init_logging_second_call_returns_none() {
        // The global tracing subscriber can only be set once per process.
        // A second call to init_logging must hit the try_init Err arm and
        // return None — verifying that the Err arm drops the guard and returns
        // None rather than falling through to Some(guard).
        //
        // NOTE: this test installs a global tracing subscriber for the rest of
        // the test binary. All other file_logging tests avoid calling init_logging
        // so this is safe, but be aware when adding future tests.
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("VK_FILE_LOGGING", "true");
            std::env::set_var("VK_LOG_DIR", temp_dir().to_str().unwrap());
        }
        // First call — may or may not succeed depending on test execution order
        let _first = init_logging("warn");
        // Second call — guaranteed to hit the Err arm (subscriber already set)
        let second = init_logging("warn");
        assert!(
            second.is_none(),
            "second init_logging call must return None when try_init fails"
        );
        unsafe {
            std::env::remove_var("VK_FILE_LOGGING");
            std::env::remove_var("VK_LOG_DIR");
        }
    }

    #[test]
    fn buffer_lines_defaults_to_128000() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::remove_var("VK_LOG_BUFFER_LINES");
        }
        let config = FileLoggingConfig::from_env(temp_dir());
        assert_eq!(config.buffer_lines, 128_000);
    }

    #[test]
    fn buffer_lines_overridden_by_env() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("VK_LOG_BUFFER_LINES", "1000");
        }
        let config = FileLoggingConfig::from_env(temp_dir());
        assert_eq!(config.buffer_lines, 1000);
        unsafe {
            std::env::remove_var("VK_LOG_BUFFER_LINES");
        }
    }

    #[test]
    fn lossy_defaults_to_true() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::remove_var("VK_LOG_LOSSY");
        }
        let config = FileLoggingConfig::from_env(temp_dir());
        assert!(config.lossy);
    }

    #[test]
    fn lossy_disabled_by_env() {
        let _lock = ENV_LOCK.lock().unwrap();
        for val in &["false", "0"] {
            unsafe {
                std::env::set_var("VK_LOG_LOSSY", val);
            }
            let config = FileLoggingConfig::from_env(temp_dir());
            assert!(!config.lossy, "expected lossy=false for VK_LOG_LOSSY={val}");
        }
        unsafe {
            std::env::remove_var("VK_LOG_LOSSY");
        }
    }

    #[test]
    fn lossy_enabled_by_other_values() {
        // Only "false" and "0" disable lossy; everything else (including typos)
        // must keep lossy enabled, matching the denylist semantics.
        let _lock = ENV_LOCK.lock().unwrap();
        for val in &["true", "1", "yes", "on", "flase", ""] {
            unsafe {
                std::env::set_var("VK_LOG_LOSSY", val);
            }
            let config = FileLoggingConfig::from_env(temp_dir());
            assert!(config.lossy, "expected lossy=true for VK_LOG_LOSSY={val}");
        }
        unsafe {
            std::env::remove_var("VK_LOG_LOSSY");
        }
    }

    #[test]
    fn buffer_lines_zero_is_clamped_to_one() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("VK_LOG_BUFFER_LINES", "0");
        }
        let config = FileLoggingConfig::from_env(temp_dir());
        assert_eq!(config.buffer_lines, 1, "buffer_lines=0 must be clamped to 1");
        unsafe {
            std::env::remove_var("VK_LOG_BUFFER_LINES");
        }
    }

    #[test]
    fn cleanup_ignores_non_log_files() {
        let dir = temp_dir();
        fs::write(dir.join("vibe-kanban.log.2025-01-01"), b"log").unwrap();
        fs::write(dir.join("unrelated.txt"), b"other").unwrap();

        cleanup_old_logs(&dir, 1); // keep 1 log file; unrelated.txt must never be touched

        assert!(dir.join("unrelated.txt").exists(), "non-log file was deleted");
        assert!(
            dir.join("vibe-kanban.log.2025-01-01").exists(),
            "log file was deleted despite being the only one"
        );
    }
}
