//! Database backup utilities for pre-migration safety.
//!
//! Creates timestamped backups before migrations run, ensuring recovery is possible
//! if a migration causes data loss or corruption.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use ts_rs::TS;

/// Information about a database backup file.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BackupInfo {
    /// Filename of the backup (e.g., "db_backup_20250101_100000.sqlite")
    pub filename: String,
    /// When the backup was created
    #[ts(type = "Date")]
    pub created_at: DateTime<Utc>,
    /// Size of the backup file in bytes
    pub size_bytes: u64,
}

/// Number of backups to retain (older ones are automatically deleted)
const DEFAULT_BACKUP_RETENTION: usize = 5;

/// Service for managing database backups
pub struct BackupService;

impl BackupService {
    /// Create a timestamped backup of the database before migrations.
    ///
    /// Returns the path to the backup file if created, or None if no database exists yet.
    pub fn backup_before_migration(db_path: &Path) -> Result<Option<std::path::PathBuf>, std::io::Error> {
        if !db_path.exists() {
            info!("No existing database to backup - skipping pre-migration backup");
            return Ok(None);
        }

        let backup_dir = db_path
            .parent()
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid database path")
            })?
            .join("backups");
        std::fs::create_dir_all(&backup_dir)?;

        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let backup_name = format!("db_backup_{}.sqlite", timestamp);
        let backup_path = backup_dir.join(&backup_name);

        // Copy main database file
        std::fs::copy(db_path, &backup_path)?;

        // Also backup WAL if exists (for complete database state)
        let wal_path = db_path.with_extension("sqlite-wal");
        if wal_path.exists() {
            let wal_backup = backup_dir.join(format!("db_backup_{}.sqlite-wal", timestamp));
            std::fs::copy(&wal_path, &wal_backup)?;
        }

        // Also backup SHM if exists
        let shm_path = db_path.with_extension("sqlite-shm");
        if shm_path.exists() {
            let shm_backup = backup_dir.join(format!("db_backup_{}.sqlite-shm", timestamp));
            std::fs::copy(&shm_path, &shm_backup)?;
        }

        info!(backup_path = %backup_path.display(), "Pre-migration database backup created");
        Ok(Some(backup_path))
    }

    /// Clean up old backups, keeping only the most recent N.
    ///
    /// Uses default retention count of 5 backups.
    pub fn cleanup_old_backups(db_path: &Path) -> Result<(), std::io::Error> {
        Self::cleanup_old_backups_with_retention(db_path, DEFAULT_BACKUP_RETENTION)
    }

    /// Clean up old backups with custom retention count.
    pub fn cleanup_old_backups_with_retention(
        db_path: &Path,
        keep_count: usize,
    ) -> Result<(), std::io::Error> {
        let backup_dir = match db_path.parent() {
            Some(parent) => parent.join("backups"),
            None => return Ok(()),
        };

        if !backup_dir.exists() {
            return Ok(());
        }

        // Collect all backup files (main .sqlite files only, not WAL/SHM)
        let mut backups: Vec<_> = std::fs::read_dir(&backup_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                path.extension().is_some_and(|ext| ext == "sqlite")
                    && path
                        .file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with("db_backup_"))
            })
            .collect();

        // Sort by modification time (newest first)
        backups.sort_by(|a, b| {
            let a_time = a.metadata().and_then(|m| m.modified()).ok();
            let b_time = b.metadata().and_then(|m| m.modified()).ok();
            b_time.cmp(&a_time)
        });

        // Remove backups beyond the retention count
        for old_backup in backups.into_iter().skip(keep_count) {
            let path = old_backup.path();

            // Remove main backup file
            if let Err(e) = std::fs::remove_file(&path) {
                warn!(path = %path.display(), error = ?e, "Failed to remove old backup");
                continue;
            }

            // Also remove associated WAL file if exists
            let wal = path.with_extension("sqlite-wal");
            if wal.exists() {
                let _ = std::fs::remove_file(&wal);
            }

            // Also remove associated SHM file if exists
            let shm = path.with_extension("sqlite-shm");
            if shm.exists() {
                let _ = std::fs::remove_file(&shm);
            }

            info!(path = %path.display(), "Removed old backup");
        }

        Ok(())
    }

    /// List all available backup files, sorted by modification time (newest first).
    ///
    /// Returns information about each backup including filename, creation time, and size.
    pub fn list_backups(db_path: &Path) -> Result<Vec<BackupInfo>, std::io::Error> {
        let backup_dir = db_path
            .parent()
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid database path")
            })?
            .join("backups");

        if !backup_dir.exists() {
            return Ok(vec![]);
        }

        let mut backups: Vec<BackupInfo> = std::fs::read_dir(&backup_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                path.extension().is_some_and(|ext| ext == "sqlite")
                    && path
                        .file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with("db_backup_"))
            })
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                Some(BackupInfo {
                    filename: e.file_name().to_string_lossy().to_string(),
                    created_at: DateTime::from(meta.modified().ok()?),
                    size_bytes: meta.len(),
                })
            })
            .collect();

        // Sort by created_at descending (newest first)
        backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(backups)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_backup_nonexistent_database() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("nonexistent.sqlite");

        let result = BackupService::backup_before_migration(&db_path).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_backup_existing_database() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");

        // Create a fake database file
        std::fs::write(&db_path, b"test database content").unwrap();

        let result = BackupService::backup_before_migration(&db_path).unwrap();
        assert!(result.is_some());

        let backup_path = result.unwrap();
        assert!(backup_path.exists());
        assert!(backup_path.to_string_lossy().contains("db_backup_"));
    }

    #[test]
    fn test_cleanup_old_backups() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");
        let backup_dir = temp_dir.path().join("backups");
        std::fs::create_dir_all(&backup_dir).unwrap();

        // Create 7 fake backup files
        for i in 0..7 {
            let backup_name = format!("db_backup_2025010{}_120000.sqlite", i);
            std::fs::write(backup_dir.join(&backup_name), format!("backup {}", i)).unwrap();
            // Small delay to ensure different modification times
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Cleanup keeping only 3
        BackupService::cleanup_old_backups_with_retention(&db_path, 3).unwrap();

        // Count remaining backups
        let remaining: Vec<_> = std::fs::read_dir(&backup_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();

        assert_eq!(remaining.len(), 3);
    }

    #[test]
    fn test_list_backups_empty() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");

        // No backup directory exists yet
        let result = BackupService::list_backups(&db_path).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_list_backups_sorted_newest_first() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");
        let backup_dir = temp_dir.path().join("backups");
        std::fs::create_dir_all(&backup_dir).unwrap();

        // Create backup files with different timestamps
        std::fs::write(
            backup_dir.join("db_backup_20250101_100000.sqlite"),
            "old backup",
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(
            backup_dir.join("db_backup_20250102_100000.sqlite"),
            "new backup",
        )
        .unwrap();

        let result = BackupService::list_backups(&db_path).unwrap();

        assert_eq!(result.len(), 2);
        // Newest first (sorted by modification time)
        assert!(result[0].filename.contains("20250102"));
        assert!(result[1].filename.contains("20250101"));
    }

    #[test]
    fn test_list_backups_ignores_non_backup_files() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");
        let backup_dir = temp_dir.path().join("backups");
        std::fs::create_dir_all(&backup_dir).unwrap();

        // Create a valid backup file
        std::fs::write(
            backup_dir.join("db_backup_20250101_100000.sqlite"),
            "valid backup",
        )
        .unwrap();

        // Create files that should be ignored
        std::fs::write(backup_dir.join("random_file.sqlite"), "random").unwrap();
        std::fs::write(backup_dir.join("db_backup_20250101_100000.txt"), "wrong ext").unwrap();
        std::fs::write(backup_dir.join("other.db"), "other").unwrap();

        let result = BackupService::list_backups(&db_path).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].filename.contains("db_backup_"));
    }
}
