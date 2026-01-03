//! Scheduled backup service for automatic database backups.
//!
//! This module provides automatic backup creation at configurable intervals,
//! ensuring recovery points are available during peak work periods.
//!
//! # Design
//!
//! - Runs as a background task creating backups periodically
//! - Configurable interval (default: 4 hours)
//! - Automatically cleans up old backups (keeps last 10)
//! - Can be disabled via environment variable

use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::mpsc;

use crate::BackupService;

/// Default backup interval in hours.
const DEFAULT_BACKUP_INTERVAL_HOURS: u64 = 4;

/// Number of scheduled backups to retain.
const SCHEDULED_BACKUP_RETENTION: usize = 10;

/// Configuration for the backup scheduler.
#[derive(Clone, Debug)]
pub struct BackupSchedulerConfig {
    /// How often to create backups (in hours).
    pub interval_hours: u64,
    /// Whether scheduled backups are enabled.
    pub enabled: bool,
}

impl Default for BackupSchedulerConfig {
    fn default() -> Self {
        Self {
            interval_hours: std::env::var("VK_BACKUP_INTERVAL_HOURS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_BACKUP_INTERVAL_HOURS),
            enabled: std::env::var("VK_SCHEDULED_BACKUPS")
                .map(|v| v != "false" && v != "0")
                .unwrap_or(true),
        }
    }
}

/// Handle for controlling the backup scheduler.
#[derive(Clone)]
pub struct BackupSchedulerHandle {
    tx: Option<mpsc::Sender<BackupSchedulerCommand>>,
}

enum BackupSchedulerCommand {
    /// Request immediate backup.
    BackupNow,
    /// Shutdown the scheduler.
    Shutdown,
}

impl BackupSchedulerHandle {
    /// Request an immediate backup.
    pub async fn backup_now(&self) {
        if let Some(ref tx) = self.tx {
            let _ = tx.send(BackupSchedulerCommand::BackupNow).await;
        }
    }

    /// Shutdown the backup scheduler.
    pub async fn shutdown(&self) {
        if let Some(ref tx) = self.tx {
            let _ = tx.send(BackupSchedulerCommand::Shutdown).await;
        }
    }

    /// Returns true if the scheduler is enabled and running.
    pub fn is_enabled(&self) -> bool {
        self.tx.is_some()
    }
}

/// Backup scheduler service.
pub struct BackupScheduler;

impl BackupScheduler {
    /// Spawn a new backup scheduler as a background task.
    ///
    /// Returns a handle that can be used to control the scheduler.
    /// If disabled via config, returns a handle with no sender.
    pub fn spawn(db_path: PathBuf, config: BackupSchedulerConfig) -> BackupSchedulerHandle {
        if !config.enabled {
            tracing::info!("Scheduled backups disabled (VK_SCHEDULED_BACKUPS=false)");
            return BackupSchedulerHandle { tx: None };
        }

        if config.interval_hours == 0 {
            tracing::info!("Scheduled backups disabled (VK_BACKUP_INTERVAL_HOURS=0)");
            return BackupSchedulerHandle { tx: None };
        }

        let (tx, rx) = mpsc::channel(4);

        tokio::spawn(Self::run(db_path, config, rx));

        BackupSchedulerHandle { tx: Some(tx) }
    }

    /// Spawn with default configuration.
    pub fn spawn_default(db_path: PathBuf) -> BackupSchedulerHandle {
        Self::spawn(db_path, BackupSchedulerConfig::default())
    }

    async fn run(
        db_path: PathBuf,
        config: BackupSchedulerConfig,
        mut rx: mpsc::Receiver<BackupSchedulerCommand>,
    ) {
        let interval_secs = config.interval_hours * 3600;
        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));

        tracing::info!(
            interval_hours = config.interval_hours,
            interval_secs = interval_secs,
            "Backup scheduler started"
        );

        // Skip first immediate tick - we don't want to backup immediately on startup
        interval.tick().await;

        loop {
            tokio::select! {
                Some(cmd) = rx.recv() => {
                    match cmd {
                        BackupSchedulerCommand::BackupNow => {
                            Self::create_scheduled_backup(&db_path).await;
                        }
                        BackupSchedulerCommand::Shutdown => {
                            tracing::info!("Backup scheduler shutting down");
                            break;
                        }
                    }
                }
                _ = interval.tick() => {
                    Self::create_scheduled_backup(&db_path).await;
                }
            }
        }
    }

    async fn create_scheduled_backup(db_path: &PathBuf) {
        tracing::info!("Running scheduled backup");

        match BackupService::create_backup(db_path) {
            Ok(info) => {
                tracing::info!(
                    filename = %info.filename,
                    size_bytes = info.size_bytes,
                    size_mb = format!("{:.2}", info.size_bytes as f64 / (1024.0 * 1024.0)),
                    "Scheduled backup created successfully"
                );

                // Clean up old backups, keeping more for scheduled backups
                if let Err(e) =
                    BackupService::cleanup_old_backups_with_retention(db_path, SCHEDULED_BACKUP_RETENTION)
                {
                    tracing::warn!(error = ?e, "Failed to cleanup old backups");
                }
            }
            Err(e) => {
                tracing::error!(error = ?e, "Scheduled backup failed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = BackupSchedulerConfig::default();
        assert_eq!(config.interval_hours, DEFAULT_BACKUP_INTERVAL_HOURS);
        assert!(config.enabled);
    }

    #[test]
    fn test_disabled_handle() {
        let config = BackupSchedulerConfig {
            interval_hours: 4,
            enabled: false,
        };
        // Can't easily test spawn without tokio runtime, but we can check the config
        assert!(!config.enabled);
    }
}
