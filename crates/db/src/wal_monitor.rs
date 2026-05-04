//! WAL (Write-Ahead Log) file monitoring service.
//!
//! This module monitors the SQLite WAL file size and triggers alerts when
//! it grows beyond acceptable thresholds. Large WAL files can indicate
//! checkpoint issues or sustained heavy write load.
//!
//! # Design
//!
//! - Runs as a background task checking WAL size periodically
//! - Logs warnings when WAL exceeds configurable threshold
//! - Optionally triggers passive checkpoint when WAL is large
//! - Runs periodic TRUNCATE checkpoints to minimize data loss on abrupt shutdown

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use sqlx::{Pool, Sqlite};
use tokio::sync::mpsc;

/// Default check interval in seconds.
const DEFAULT_CHECK_INTERVAL_SECS: u64 = 60;

/// Default WAL size warning threshold in MB.
const DEFAULT_WARNING_THRESHOLD_MB: u64 = 50;

/// Default WAL size for triggering passive checkpoint in MB.
const DEFAULT_CHECKPOINT_THRESHOLD_MB: u64 = 100;

/// Default interval for forced TRUNCATE checkpoint in seconds (5 minutes).
/// This ensures max data loss of 5 minutes if the server is killed abruptly.
const DEFAULT_TRUNCATE_INTERVAL_SECS: u64 = 300;

/// Configuration for the WAL monitor.
#[derive(Clone, Debug)]
pub struct WalMonitorConfig {
    /// How often to check WAL size (in seconds).
    pub check_interval_secs: u64,
    /// WAL size in bytes that triggers a warning log.
    pub warning_threshold_bytes: u64,
    /// WAL size in bytes that triggers a passive checkpoint.
    pub checkpoint_threshold_bytes: u64,
    /// Whether to automatically trigger passive checkpoints.
    pub auto_checkpoint: bool,
    /// Interval in seconds for forced TRUNCATE checkpoint (flushes all WAL to main DB).
    /// This ensures data is regularly persisted to minimize loss on abrupt kill.
    /// Set to 0 to disable periodic TRUNCATE checkpoints.
    pub truncate_checkpoint_interval_secs: u64,
}

impl Default for WalMonitorConfig {
    fn default() -> Self {
        let warning_mb =
            get_env_or_default("VK_WAL_WARNING_THRESHOLD_MB", DEFAULT_WARNING_THRESHOLD_MB);
        let checkpoint_mb = get_env_or_default(
            "VK_WAL_CHECKPOINT_THRESHOLD_MB",
            DEFAULT_CHECKPOINT_THRESHOLD_MB,
        );

        Self {
            check_interval_secs: get_env_or_default(
                "VK_WAL_CHECK_INTERVAL_SECS",
                DEFAULT_CHECK_INTERVAL_SECS,
            ),
            warning_threshold_bytes: warning_mb * 1024 * 1024,
            checkpoint_threshold_bytes: checkpoint_mb * 1024 * 1024,
            auto_checkpoint: std::env::var("VK_WAL_AUTO_CHECKPOINT")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(true),
            truncate_checkpoint_interval_secs: get_env_or_default(
                "VK_WAL_TRUNCATE_INTERVAL_SECS",
                DEFAULT_TRUNCATE_INTERVAL_SECS,
            ),
        }
    }
}

fn get_env_or_default(var: &str, default: u64) -> u64 {
    std::env::var(var)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// Handle for controlling the WAL monitor.
#[derive(Clone)]
pub struct WalMonitorHandle {
    tx: mpsc::Sender<WalMonitorCommand>,
}

enum WalMonitorCommand {
    /// Request immediate WAL size check.
    CheckNow,
    /// Request immediate passive checkpoint.
    Checkpoint,
    /// Request immediate TRUNCATE checkpoint (blocks until all WAL is flushed).
    TruncateCheckpoint,
    /// Shutdown the monitor.
    Shutdown,
}

impl WalMonitorHandle {
    /// Request an immediate WAL size check.
    pub async fn check_now(&self) {
        let _ = self.tx.send(WalMonitorCommand::CheckNow).await;
    }

    /// Request an immediate passive checkpoint.
    pub async fn checkpoint(&self) {
        let _ = self.tx.send(WalMonitorCommand::Checkpoint).await;
    }

    /// Request an immediate TRUNCATE checkpoint (blocks until all WAL is flushed).
    pub async fn truncate_checkpoint(&self) {
        let _ = self.tx.send(WalMonitorCommand::TruncateCheckpoint).await;
    }

    /// Shutdown the WAL monitor.
    pub async fn shutdown(&self) {
        let _ = self.tx.send(WalMonitorCommand::Shutdown).await;
    }
}

/// WAL monitoring service.
pub struct WalMonitor {
    db_path: PathBuf,
    pool: Pool<Sqlite>,
    config: WalMonitorConfig,
}

impl WalMonitor {
    /// Spawn a new WAL monitor as a background task with default configuration.
    ///
    /// Returns a handle that can be used to control the monitor.
    pub fn spawn(pool: Pool<Sqlite>, db_path: PathBuf) -> WalMonitorHandle {
        let (tx, rx) = mpsc::channel(16);
        let monitor = Self {
            db_path,
            pool,
            config: WalMonitorConfig::default(),
        };
        tokio::spawn(monitor.run(rx));
        WalMonitorHandle { tx }
    }

    async fn run(self, mut rx: mpsc::Receiver<WalMonitorCommand>) {
        let mut check_interval =
            tokio::time::interval(Duration::from_secs(self.config.check_interval_secs));

        // Periodic TRUNCATE checkpoint timer - ensures data is persisted regularly
        // to minimize data loss if the server is killed abruptly.
        // Uses Option<Interval> so the select! arm is cleanly disabled when not configured.
        let mut truncate_interval: Option<tokio::time::Interval> =
            if self.config.truncate_checkpoint_interval_secs > 0 {
                let mut i = tokio::time::interval(Duration::from_secs(
                    self.config.truncate_checkpoint_interval_secs,
                ));
                i.tick().await; // consume immediate first tick
                Some(i)
            } else {
                None
            };

        tracing::info!(
            check_interval_secs = self.config.check_interval_secs,
            warning_threshold_mb = self.config.warning_threshold_bytes / (1024 * 1024),
            checkpoint_threshold_mb = self.config.checkpoint_threshold_bytes / (1024 * 1024),
            auto_checkpoint = self.config.auto_checkpoint,
            truncate_interval_secs = self.config.truncate_checkpoint_interval_secs,
            "WAL monitor started"
        );

        loop {
            tokio::select! {
                Some(cmd) = rx.recv() => {
                    match cmd {
                        WalMonitorCommand::CheckNow => {
                            self.check_wal_size().await;
                        }
                        WalMonitorCommand::Checkpoint => {
                            self.run_checkpoint().await;
                        }
                        WalMonitorCommand::TruncateCheckpoint => {
                            self.run_truncate_checkpoint().await;
                        }
                        WalMonitorCommand::Shutdown => {
                            tracing::info!("WAL monitor shutting down");
                            break;
                        }
                    }
                }
                _ = check_interval.tick() => {
                    self.check_wal_size().await;
                }
                _ = async {
                    if let Some(ref mut t) = truncate_interval {
                        t.tick().await
                    } else {
                        std::future::pending::<tokio::time::Instant>().await
                    }
                } => {
                    self.run_truncate_checkpoint().await;
                }
            }
        }
    }

    async fn check_wal_size(&self) {
        let wal_path = self.db_path.with_extension("sqlite-wal");

        let wal_size = match std::fs::metadata(&wal_path) {
            Ok(meta) => meta.len(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // WAL file doesn't exist (might be using different journal mode)
                0
            }
            Err(e) => {
                tracing::warn!(
                    error = ?e,
                    path = ?wal_path,
                    "Failed to read WAL file metadata"
                );
                return;
            }
        };

        let wal_size_mb = wal_size as f64 / (1024.0 * 1024.0);

        if wal_size >= self.config.checkpoint_threshold_bytes {
            tracing::warn!(
                wal_size_mb = format!("{:.2}", wal_size_mb),
                threshold_mb = self.config.checkpoint_threshold_bytes / (1024 * 1024),
                "WAL file exceeds checkpoint threshold"
            );

            if self.config.auto_checkpoint {
                self.run_checkpoint().await;
            }
        } else if wal_size >= self.config.warning_threshold_bytes {
            tracing::warn!(
                wal_size_mb = format!("{:.2}", wal_size_mb),
                threshold_mb = self.config.warning_threshold_bytes / (1024 * 1024),
                "WAL file size exceeds warning threshold"
            );
        } else {
            tracing::debug!(
                wal_size_mb = format!("{:.2}", wal_size_mb),
                "WAL file size check completed"
            );
        }
    }

    /// Run a PASSIVE checkpoint.
    ///
    /// PASSIVE checkpoint does not block readers or writers.
    /// It checkpoints as many frames as possible without waiting.
    async fn run_checkpoint(&self) {
        tracing::info!("Running passive WAL checkpoint");

        let start = std::time::Instant::now();

        let result: Result<(i32, i32, i32), sqlx::Error> =
            sqlx::query_as("PRAGMA wal_checkpoint(PASSIVE)")
                .fetch_one(&self.pool)
                .await;

        let duration = start.elapsed();

        match result {
            Ok((blocked, log_pages, checkpointed)) => {
                tracing::info!(
                    duration_ms = duration.as_millis() as u64,
                    blocked = blocked,
                    log_pages = log_pages,
                    checkpointed = checkpointed,
                    "WAL checkpoint completed"
                );
            }
            Err(e) => {
                tracing::error!(
                    error = ?e,
                    duration_ms = duration.as_millis() as u64,
                    "WAL checkpoint failed"
                );
            }
        }
    }

    /// Run a TRUNCATE checkpoint.
    ///
    /// TRUNCATE checkpoint blocks until ALL WAL content is written to the main database file,
    /// then truncates the WAL file to zero bytes. This ensures all data is persisted to the
    /// main database file, minimizing data loss if the server is killed abruptly.
    ///
    /// If the TRUNCATE is busy or incomplete (active readers/writers), falls back to a
    /// PASSIVE checkpoint and logs a warning.
    async fn run_truncate_checkpoint(&self) {
        tracing::info!("Running TRUNCATE checkpoint (periodic data safety)");

        let start = std::time::Instant::now();

        let result: Result<(i32, i32, i32), sqlx::Error> =
            sqlx::query_as("PRAGMA wal_checkpoint(TRUNCATE)")
                .fetch_one(&self.pool)
                .await;

        let duration = start.elapsed();

        match result {
            Ok((blocked, log_pages, checkpointed)) => {
                if blocked == 0 {
                    tracing::info!(
                        duration_ms = duration.as_millis() as u64,
                        log_pages = log_pages,
                        checkpointed = checkpointed,
                        "TRUNCATE checkpoint completed - all WAL flushed to main database"
                    );
                } else {
                    // blocked != 0 is SQLite's indication that readers/writers prevented
                    // full checkpointing — this is the WAL-mode equivalent of SQLITE_BUSY
                    // for TRUNCATE mode. The WAL was not fully flushed; fall back to a
                    // PASSIVE checkpoint rather than treating this as success.
                    tracing::warn!(
                        duration_ms = duration.as_millis() as u64,
                        blocked = blocked,
                        log_pages = log_pages,
                        checkpointed = checkpointed,
                        "TRUNCATE checkpoint was blocked - falling back to PASSIVE checkpoint"
                    );
                    self.run_checkpoint().await;
                }
            }
            Err(e) => {
                tracing::warn!(
                    error = ?e,
                    duration_ms = duration.as_millis() as u64,
                    "TRUNCATE checkpoint failed - falling back to PASSIVE checkpoint"
                );
                self.run_checkpoint().await;
            }
        }
    }
}

/// Get the current WAL file size for a database.
///
/// Returns 0 if the WAL file doesn't exist.
pub fn get_wal_size(db_path: impl AsRef<Path>) -> u64 {
    let wal_path = db_path.as_ref().with_extension("sqlite-wal");
    std::fs::metadata(&wal_path).map(|m| m.len()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = WalMonitorConfig::default();
        assert_eq!(config.check_interval_secs, DEFAULT_CHECK_INTERVAL_SECS);
        assert_eq!(
            config.warning_threshold_bytes,
            DEFAULT_WARNING_THRESHOLD_MB * 1024 * 1024
        );
        assert_eq!(
            config.checkpoint_threshold_bytes,
            DEFAULT_CHECKPOINT_THRESHOLD_MB * 1024 * 1024
        );
        assert!(config.auto_checkpoint);
        assert_eq!(
            config.truncate_checkpoint_interval_secs,
            DEFAULT_TRUNCATE_INTERVAL_SECS
        );
    }

    #[test]
    fn test_get_wal_size_nonexistent() {
        let size = get_wal_size("/nonexistent/path/db.sqlite");
        assert_eq!(size, 0);
    }
}
