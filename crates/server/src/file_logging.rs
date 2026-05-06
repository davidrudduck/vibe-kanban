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

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};
use utils::{assets::asset_dir, sentry::sentry_layer};

#[derive(Clone)]
pub struct FileLoggingConfig {
    pub enabled: bool,
    pub log_dir: PathBuf,
    pub max_files: usize,
    /// Maximum number of log lines buffered before the non-blocking writer
    /// either drops (lossy=true) or blocks (lossy=false). Default: 128_000.
    pub buffer_lines: usize,
    /// When `true` (default), excess log lines are dropped under load rather
    /// than blocking the application.
    /// Set `VK_LOG_LOSSY` to `false`, `0`, `no`, or `off` (case-insensitive) to block instead.
    ///
    /// Caution: `lossy=false` can block tokio workers under log burst; use only
    /// for debugging on non-production deployments.
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
        } else if raw_buffer > MAX_BUFFER_LINES {
            eprintln!("VK_LOG_BUFFER_LINES={raw_buffer} exceeds maximum ({MAX_BUFFER_LINES}); using {MAX_BUFFER_LINES}");
            MAX_BUFFER_LINES
        } else {
            raw_buffer
        };

        let lossy = std::env::var("VK_LOG_LOSSY")
            .map(|v| {
                let v = v.to_lowercase();
                v != "false" && v != "0" && v != "no" && v != "off"
            })
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

/// Holds the non-blocking writer guard and optionally the config needed to
/// spawn a periodic cleanup task.
///
/// **Hold this value for the entire lifetime of the process.** Dropping it
/// flushes and stops the background file-writer thread.
/// Maximum non-blocking writer buffer capacity.
/// At ~200 bytes/line this caps in-process queue memory at ~200 MB.
const MAX_BUFFER_LINES: usize = 1_000_000;

/// Maximum time to wait for the log cleanup task to finish during shutdown.
const CLEANUP_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

pub struct LoggingHandle {
    /// Held purely for RAII: dropping flushes buffered lines and stops the writer thread.
    _guard: Option<WorkerGuard>,
    /// Present when file logging was successfully initialised; used to spawn cleanup.
    pub cleanup_config: Option<FileLoggingConfig>,
    /// JoinHandle for the daily cleanup task; set by `spawn_cleanup_task`.
    cleanup_task: Option<JoinHandle<()>>,
}

impl LoggingHandle {
    fn console_only() -> Self {
        Self {
            _guard: None,
            cleanup_config: None,
            cleanup_task: None,
        }
    }

    fn with_file(guard: WorkerGuard, config: FileLoggingConfig) -> Self {
        Self {
            _guard: Some(guard),
            cleanup_config: Some(config),
            cleanup_task: None,
        }
    }

    /// Spawn a background tokio task that runs `cleanup_old_logs` once per day.
    ///
    /// Call this after the tokio runtime is running (i.e. inside an `async fn`).
    /// The task exits when `shutdown` is cancelled. Await completion via
    /// [`wait_for_cleanup_task`] before the process exits.
    ///
    /// No-op if file logging is not active.
    pub fn spawn_cleanup_task(&mut self, shutdown: CancellationToken) {
        if let Some(ref config) = self.cleanup_config {
            let config = config.clone();
            self.cleanup_task = Some(tokio::spawn(run_cleanup_loop(config, shutdown)));
        }
    }

    /// Wait for the cleanup task to finish (up to 5 seconds after cancellation).
    ///
    /// Call this after `shutdown_token.cancel()` and before the process exits.
    /// No-op if no cleanup task was spawned.
    pub async fn wait_for_cleanup_task(&mut self) {
        if let Some(task) = self.cleanup_task.take() {
            match tokio::time::timeout(CLEANUP_SHUTDOWN_TIMEOUT, task).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => eprintln!("Log cleanup task panicked: {e}"),
                Err(_) => {
                    eprintln!("Log cleanup task did not finish within 5s; continuing shutdown")
                }
            }
        }
    }
}

/// Runs `cleanup_old_logs` once per day until `shutdown` is cancelled.
///
/// Filesystem I/O is offloaded to `tokio::task::spawn_blocking` so cleanup
/// never blocks the async runtime's worker threads.
pub(crate) async fn run_cleanup_loop(config: FileLoggingConfig, shutdown: CancellationToken) {
    const ONE_DAY: Duration = Duration::from_secs(24 * 60 * 60);
    // Run once immediately at startup on the blocking thread pool.
    {
        let dir = config.log_dir.clone();
        let max = config.max_files;
        let _ = tokio::task::spawn_blocking(move || cleanup_old_logs(&dir, max)).await;
    }
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => {
                tracing::debug!("Log cleanup task shutting down");
                break;
            }
            _ = tokio::time::sleep(ONE_DAY) => {
                let dir = config.log_dir.clone();
                let max = config.max_files;
                let _ = tokio::task::spawn_blocking(move || cleanup_old_logs(&dir, max)).await;
            }
        }
    }
}

/// Build the tracing filter string from the `RUST_LOG` environment variable.
///
/// If `RUST_LOG` (after trimming whitespace) contains `=` or `,` it is treated
/// as a full directive string and passed through verbatim (e.g. `"warn,hyper=error"`).
/// Otherwise it is treated as a plain level name; unrecognised names fall back to
/// `"info"` with a stderr warning rather than panicking.
pub fn build_filter_string() -> String {
    let rust_log = std::env::var("RUST_LOG").unwrap_or_default();
    let rust_log = rust_log.trim();

    if rust_log.contains('=') || rust_log.contains(',') {
        // Full directive — pass through as-is; init_logging falls back on parse error.
        rust_log.to_string()
    } else {
        const VALID_LEVELS: &[&str] = &["trace", "debug", "info", "warn", "error", "off"];
        let rust_log_lower;
        let level = if rust_log.is_empty() {
            "info"
        } else {
            rust_log_lower = rust_log.to_ascii_lowercase();
            if VALID_LEVELS.contains(&rust_log_lower.as_str()) {
                rust_log_lower.as_str()
            } else {
                eprintln!("Unrecognised RUST_LOG level '{rust_log}'; using 'info'");
                "info"
            }
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
/// If `filter_string` is not a valid `EnvFilter` directive, falls back to `"warn"`
/// and prints a diagnostic to stderr rather than panicking.
pub fn init_logging(filter_string: &str) -> LoggingHandle {
    let config = FileLoggingConfig::from_env(asset_dir());

    let env_filter = EnvFilter::try_new(filter_string).unwrap_or_else(|e| {
        eprintln!("Invalid tracing filter '{filter_string}': {e}; using 'warn'");
        EnvFilter::new("warn")
    });
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
            return LoggingHandle::console_only();
        }

        let file_appender = tracing_appender::rolling::daily(&config.log_dir, "vibe-kanban.log");
        let (non_blocking, guard) = tracing_appender::non_blocking::NonBlockingBuilder::default()
            .buffered_lines_limit(config.buffer_lines)
            .lossy(config.lossy)
            .finish(file_appender);

        let file_filter = EnvFilter::try_new(filter_string).unwrap_or_else(|e| {
            eprintln!("Invalid file tracing filter '{filter_string}': {e}; using 'warn'");
            EnvFilter::new("warn")
        });
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
            drop(guard);
            return LoggingHandle::console_only();
        }

        tracing::info!(
            log_dir = ?config.log_dir,
            max_files = config.max_files,
            buffer_lines = config.buffer_lines,
            lossy = config.lossy,
            "File logging enabled"
        );

        LoggingHandle::with_file(guard, config)
    } else {
        if let Err(e) = tracing_subscriber::registry()
            .with(console_layer)
            .with(sentry_layer())
            .try_init()
        {
            eprintln!("Tracing subscriber already initialised: {e}");
        }
        LoggingHandle::console_only()
    }
}

/// Filename prefix used by the daily rolling appender.
/// Shared between `is_log_date_suffix` and `cleanup_old_logs` to prevent drift.
const LOG_PREFIX: &str = "vibe-kanban.log.";

/// Returns `true` only for filenames matching `vibe-kanban.log.YYYY-MM-DD`.
///
/// Strict: the date part must be exactly 10 characters, ASCII digits in the
/// right positions, dashes at positions 4 and 7, month in 01–12, and day in
/// 01–31 (calendar accuracy is not required; structural validity is enough to
/// distinguish date-suffix files from `.bak`, `.old`, etc.).
pub(crate) fn is_log_date_suffix(name: &str) -> bool {
    let Some(suffix) = name.strip_prefix(LOG_PREFIX) else {
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
    // Year 2000-2099: rejects far-future dates that would block log rotation.
    let year = ((b[0] - b'0') as u32 * 1000)
        + ((b[1] - b'0') as u32 * 100)
        + ((b[2] - b'0') as u32 * 10)
        + (b[3] - b'0') as u32;
    let month = (b[5] - b'0') * 10 + (b[6] - b'0');
    let day = (b[8] - b'0') * 10 + (b[9] - b'0');
    (2000..=2099).contains(&year) && (1..=12).contains(&month) && (1..=31).contains(&day)
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
        // Only regular files — directories/symlinks/FIFOs with matching names are skipped.
        .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_owned();
            if is_log_date_suffix(&name) {
                let date = name[LOG_PREFIX.len()..].to_owned();
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
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("vk-log-test-{}-{}", std::process::id(), n));
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

        assert_eq!(
            remaining.len(),
            3,
            "expected exactly 3 files; got {:?}",
            remaining
        );
        assert!(
            remaining.contains("vibe-kanban.log.2025-01-08"),
            "day 08 missing"
        );
        assert!(
            remaining.contains("vibe-kanban.log.2025-01-09"),
            "day 09 missing"
        );
        assert!(
            remaining.contains("vibe-kanban.log.2025-01-10"),
            "day 10 missing"
        );
    }

    #[test]
    fn cleanup_rejects_non_date_suffix_files() {
        let dir = temp_dir();
        // These must NEVER be deleted — they don't match the date suffix pattern
        for name in &[
            "vibe-kanban.log",            // no date suffix
            "vibe-kanban.log.bak",        // backup extension
            "vibe-kanban.log.old",        // old extension
            "vibe-kanban.log.2025-1-1",   // wrong date format (not zero-padded)
            "vibe-kanban.log.2025-13-01", // invalid month
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
            assert!(
                dir.join(name).exists(),
                "{name} was deleted but should not be"
            );
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
            second.cleanup_config.is_none(),
            "second init_logging call must return console-only handle when try_init fails"
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
        // Only "false", "0", "no", and "off" (case-insensitive) disable lossy;
        // everything else (including typos) must keep lossy enabled.
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
    fn buffer_lines_exceeding_maximum_is_clamped() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("VK_LOG_BUFFER_LINES", "99999999");
        }
        let config = FileLoggingConfig::from_env(temp_dir());
        assert_eq!(
            config.buffer_lines, 1_000_000,
            "buffer_lines above max must be clamped to 1_000_000"
        );
        unsafe {
            std::env::remove_var("VK_LOG_BUFFER_LINES");
        }
    }

    #[test]
    fn lossy_disabled_by_env_case_insensitive() {
        let _lock = ENV_LOCK.lock().unwrap();
        for val in &["False", "FALSE", "no", "No", "off", "OFF"] {
            unsafe {
                std::env::set_var("VK_LOG_LOSSY", val);
            }
            let config = FileLoggingConfig::from_env(temp_dir());
            assert!(
                !config.lossy,
                "expected lossy=false for VK_LOG_LOSSY={val}"
            );
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
        assert_eq!(
            config.buffer_lines, 1,
            "buffer_lines=0 must be clamped to 1"
        );
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

        assert!(
            dir.join("unrelated.txt").exists(),
            "non-log file was deleted"
        );
        assert!(
            dir.join("vibe-kanban.log.2025-01-01").exists(),
            "log file was deleted despite being the only one"
        );
    }

    #[tokio::test]
    async fn logging_handle_spawn_cleanup_is_noop_when_disabled() {
        // Verify spawn_cleanup_task is a true no-op when cleanup_config is None.
        // Running inside a tokio runtime means if it accidentally spawned a task,
        // that would be visible (no panic, but the test exercises the real path).
        let mut handle = LoggingHandle {
            _guard: None,
            cleanup_config: None,
            cleanup_task: None,
        };
        let token = CancellationToken::new();
        handle.spawn_cleanup_task(token.clone()); // must not panic, must not spawn
        token.cancel();
        drop(handle);
    }

    #[tokio::test]
    async fn run_cleanup_loop_keeps_newest_with_max_files_1() {
        // Validates that run_cleanup_loop correctly offloads I/O to the
        // blocking thread pool and still cleans up files as expected.
        let dir = temp_dir();
        for i in 1..=3u32 {
            let name = format!("vibe-kanban.log.2025-03-{:02}", i);
            fs::write(dir.join(&name), b"log").unwrap();
        }

        let config = FileLoggingConfig {
            enabled: true,
            log_dir: dir.clone(),
            max_files: 1,
            buffer_lines: 128_000,
            lossy: true,
        };
        let shutdown = CancellationToken::new();
        shutdown.cancel();

        run_cleanup_loop(config, shutdown).await;

        let remaining: std::collections::HashSet<String> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();

        assert_eq!(remaining.len(), 1, "got: {:?}", remaining);
        assert!(
            remaining.contains("vibe-kanban.log.2025-03-03"),
            "newest file missing"
        );
    }

    #[test]
    fn build_filter_string_trims_whitespace() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("RUST_LOG", "  warn  ");
        }
        let s = build_filter_string();
        // Trimmed "warn" is a plain level — must be interpolated, not passed through
        assert!(s.contains("server=warn"), "got: {s}");
        assert!(!s.contains("  "), "whitespace not trimmed: {s}");
        unsafe {
            std::env::remove_var("RUST_LOG");
        }
    }

    #[test]
    fn build_filter_string_unknown_level_falls_back_to_info() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("RUST_LOG", "notavalid");
        }
        let s = build_filter_string();
        assert!(s.contains("server=info"), "got: {s}");
        unsafe {
            std::env::remove_var("RUST_LOG");
        }
    }

    #[test]
    fn build_filter_string_malformed_directive_passes_through_verbatim() {
        let _lock = ENV_LOCK.lock().unwrap();
        for val in &[",", "==", "warn,", "=debug"] {
            unsafe {
                std::env::set_var("RUST_LOG", val);
            }
            let s = build_filter_string();
            assert_eq!(&s, val, "should pass through verbatim: {val}");
        }
        unsafe {
            std::env::remove_var("RUST_LOG");
        }
    }

    #[test]
    fn build_filter_string_with_uppercase_level() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("RUST_LOG", "WARN");
        }
        let s = build_filter_string();
        assert!(s.contains("server=warn"), "uppercase WARN should work: {s}");
        unsafe {
            std::env::remove_var("RUST_LOG");
        }
    }

    #[test]
    fn cleanup_skips_directories_with_log_names() {
        let dir = temp_dir();
        // A directory named like a log file must NOT count as a retention slot.
        fs::create_dir(dir.join("vibe-kanban.log.2025-06-02")).unwrap();
        fs::write(dir.join("vibe-kanban.log.2025-06-01"), b"log").unwrap();

        // With max_files=1 the real file must survive; directory is silently skipped.
        cleanup_old_logs(&dir, 1);

        assert!(
            dir.join("vibe-kanban.log.2025-06-01").exists(),
            "real log file was deleted because directory consumed its slot"
        );

        // The directory must NOT have been removed — cleanup never deletes non-files.
        assert!(
            dir.join("vibe-kanban.log.2025-06-02").is_dir(),
            "directory was unexpectedly removed by cleanup"
        );
    }

    #[test]
    fn is_log_date_suffix_rejects_far_future_year() {
        assert!(!is_log_date_suffix("vibe-kanban.log.9999-01-01"));
        assert!(!is_log_date_suffix("vibe-kanban.log.2100-01-01"));
        // Years in valid range must still be accepted.
        assert!(is_log_date_suffix("vibe-kanban.log.2000-01-01"));
        assert!(is_log_date_suffix("vibe-kanban.log.2099-12-31"));
    }

    #[tokio::test]
    async fn run_cleanup_loop_cleans_on_startup() {
        // run_cleanup_loop runs cleanup_old_logs immediately before entering
        // the select! loop. Cancel the token immediately to verify that the
        // startup cleanup ran (without waiting 24h for the timer to fire).
        let dir = temp_dir();
        for i in 1..=5u32 {
            let name = format!("vibe-kanban.log.2025-02-{:02}", i);
            fs::write(dir.join(&name), b"log").unwrap();
        }

        let config = FileLoggingConfig {
            enabled: true,
            log_dir: dir.clone(),
            max_files: 2,
            buffer_lines: 128_000,
            lossy: true,
        };
        let shutdown = CancellationToken::new();
        shutdown.cancel(); // cancel before the loop so only startup cleanup runs

        run_cleanup_loop(config, shutdown).await;

        let remaining: std::collections::HashSet<String> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();

        assert_eq!(
            remaining.len(),
            2,
            "startup cleanup should keep 2 newest; got: {:?}",
            remaining
        );
        assert!(
            remaining.contains("vibe-kanban.log.2025-02-04"),
            "day 04 missing"
        );
        assert!(
            remaining.contains("vibe-kanban.log.2025-02-05"),
            "day 05 missing"
        );
    }
}
