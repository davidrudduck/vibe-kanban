//! Database backup utilities for pre-migration safety.
//!
//! Creates timestamped backups before migrations run, ensuring recovery is possible
//! if a migration causes data loss or corruption.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use ts_rs::TS;
use utils::assets::backup_dir;

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
    pub fn backup_before_migration(
        db_path: &Path,
    ) -> Result<Option<std::path::PathBuf>, std::io::Error> {
        if !db_path.exists() {
            info!("No existing database to backup - skipping pre-migration backup");
            return Ok(None);
        }

        let backup_directory = backup_dir();
        std::fs::create_dir_all(&backup_directory)?;

        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let backup_name = format!("db_backup_{}.sqlite", timestamp);
        let backup_path = backup_directory.join(&backup_name);

        // Copy main database file
        std::fs::copy(db_path, &backup_path)?;

        // Also backup WAL if exists (for complete database state)
        let wal_path = db_path.with_extension("sqlite-wal");
        if wal_path.exists() {
            let wal_backup = backup_directory.join(format!("db_backup_{}.sqlite-wal", timestamp));
            std::fs::copy(&wal_path, &wal_backup)?;
        }

        // Also backup SHM if exists
        let shm_path = db_path.with_extension("sqlite-shm");
        if shm_path.exists() {
            let shm_backup = backup_directory.join(format!("db_backup_{}.sqlite-shm", timestamp));
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
        _db_path: &Path,
        keep_count: usize,
    ) -> Result<(), std::io::Error> {
        let backup_directory = backup_dir();

        if !backup_directory.exists() {
            return Ok(());
        }

        // Collect all backup files (main .sqlite files only, not WAL/SHM)
        let mut backups: Vec<_> = std::fs::read_dir(&backup_directory)?
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

    /// Create a new backup of the database.
    ///
    /// Creates a timestamped backup in the backups directory and returns information about it.
    /// Also backs up WAL and SHM files if they exist.
    pub fn create_backup(db_path: &Path) -> Result<BackupInfo, std::io::Error> {
        if !db_path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Database not found",
            ));
        }

        let backup_directory = backup_dir();
        std::fs::create_dir_all(&backup_directory)?;

        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("db_backup_{}.sqlite", timestamp);
        let backup_path = backup_directory.join(&filename);

        // Copy main database file
        std::fs::copy(db_path, &backup_path)?;

        // Also backup WAL if exists
        let wal_path = db_path.with_extension("sqlite-wal");
        if wal_path.exists() {
            let wal_backup = backup_directory.join(format!("db_backup_{}.sqlite-wal", timestamp));
            std::fs::copy(&wal_path, &wal_backup)?;
        }

        // Also backup SHM if exists
        let shm_path = db_path.with_extension("sqlite-shm");
        if shm_path.exists() {
            let shm_backup = backup_directory.join(format!("db_backup_{}.sqlite-shm", timestamp));
            std::fs::copy(&shm_path, &shm_backup)?;
        }

        let meta = std::fs::metadata(&backup_path)?;
        info!(backup_path = %backup_path.display(), "Database backup created");

        Ok(BackupInfo {
            filename,
            created_at: Utc::now(),
            size_bytes: meta.len(),
        })
    }

    /// List all available backup files, sorted by modification time (newest first).
    ///
    /// Returns information about each backup including filename, creation time, and size.
    pub fn list_backups(_db_path: &Path) -> Result<Vec<BackupInfo>, std::io::Error> {
        let backup_directory = backup_dir();

        if !backup_directory.exists() {
            return Ok(vec![]);
        }

        let mut backups: Vec<BackupInfo> = std::fs::read_dir(&backup_directory)?
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

    /// Delete a backup file by filename.
    ///
    /// Security: Validates the filename to prevent path traversal attacks.
    /// Only files matching the backup naming pattern can be deleted.
    pub fn delete_backup(_db_path: &Path, filename: &str) -> Result<(), std::io::Error> {
        // Security: validate filename pattern to prevent path traversal
        if !filename.starts_with("db_backup_")
            || !filename.ends_with(".sqlite")
            || filename.contains("..")
            || filename.contains('/')
            || filename.contains('\\')
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid backup filename",
            ));
        }

        let backup_directory = backup_dir();

        let backup_path = backup_directory.join(filename);

        if !backup_path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Backup not found",
            ));
        }

        // Remove main backup file
        std::fs::remove_file(&backup_path)?;

        // Also remove associated WAL/SHM files if they exist
        let base = filename.trim_end_matches(".sqlite");
        let _ = std::fs::remove_file(backup_directory.join(format!("{}.sqlite-wal", base)));
        let _ = std::fs::remove_file(backup_directory.join(format!("{}.sqlite-shm", base)));

        info!(filename = %filename, "Database backup deleted");
        Ok(())
    }

    /// Get the full path to a backup file by filename.
    ///
    /// Security: Validates the filename to prevent path traversal attacks.
    /// Returns an error if the file doesn't exist or the filename is invalid.
    pub fn get_backup_path(
        _db_path: &Path,
        filename: &str,
    ) -> Result<std::path::PathBuf, std::io::Error> {
        // Security: validate filename pattern to prevent path traversal
        if !filename.starts_with("db_backup_")
            || !filename.ends_with(".sqlite")
            || filename.contains("..")
            || filename.contains('/')
            || filename.contains('\\')
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid backup filename",
            ));
        }

        let backup_directory = backup_dir();

        let backup_path = backup_directory.join(filename);

        if !backup_path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Backup not found",
            ));
        }

        Ok(backup_path)
    }

    /// Restore a database from uploaded backup data.
    ///
    /// Security: Validates that the data is a valid SQLite file before restoring.
    /// Creates a backup of the current database before restoring.
    /// Removes WAL/SHM files to force a clean database state.
    pub fn restore_from_data(db_path: &Path, data: &[u8]) -> Result<(), std::io::Error> {
        // Validate SQLite header (first 16 bytes must be "SQLite format 3\0")
        if data.len() < 16 || &data[0..16] != b"SQLite format 3\0" {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid SQLite file",
            ));
        }

        // Create a backup of the current database before restoring
        if db_path.exists() {
            let _ = Self::create_backup(db_path);
        }

        // Write the restored data
        std::fs::write(db_path, data)?;

        // Remove WAL/SHM files to force a clean database state on restart
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));

        info!(db_path = %db_path.display(), "Database restored from backup");
        Ok(())
    }

    /// Find the most recent backup that has task_attempts data.
    ///
    /// Used for recovery from the broken migration 20260102051142 which accidentally
    /// deleted all task_attempts due to missing PRAGMA foreign_keys = OFF.
    ///
    /// Returns None if no backup with attempts data exists.
    pub fn find_backup_with_attempts(_db_path: &Path) -> Option<PathBuf> {
        let backup_directory = backup_dir();
        if !backup_directory.exists() {
            return None;
        }

        // Get all backups sorted by time (newest first)
        let mut backups: Vec<_> = std::fs::read_dir(&backup_directory)
            .ok()?
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                path.extension().is_some_and(|ext| ext == "sqlite")
                    && path
                        .file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with("db_backup_"))
            })
            .collect();

        backups.sort_by(|a, b| {
            let a_time = a.metadata().and_then(|m| m.modified()).ok();
            let b_time = b.metadata().and_then(|m| m.modified()).ok();
            b_time.cmp(&a_time)
        });

        // Check each backup for task_attempts data
        for backup in backups {
            let path = backup.path();
            if let Ok(conn) = rusqlite::Connection::open(&path)
                && let Ok(count) = conn.query_row::<i64, _, _>(
                    "SELECT COUNT(*) FROM task_attempts",
                    [],
                    |row| row.get(0),
                )
                && count > 0
            {
                info!(
                    backup = %path.display(),
                    attempts = count,
                    "Found backup with task_attempts data"
                );
                return Some(path);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::env;
    use tempfile::TempDir;

    /// Helper to set VK_BACKUP_DIR for tests.
    /// SAFETY: Tests must run serially via #[serial] attribute.
    fn set_backup_dir_env(path: &Path) {
        unsafe { env::set_var("VK_BACKUP_DIR", path) };
    }

    /// Helper to clear VK_BACKUP_DIR after tests.
    /// SAFETY: Tests must run serially via #[serial] attribute.
    fn clear_backup_dir_env() {
        unsafe { env::remove_var("VK_BACKUP_DIR") };
    }

    #[test]
    #[serial]
    fn test_backup_nonexistent_database() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("nonexistent.sqlite");
        let backup_directory = temp_dir.path().join("backups");
        set_backup_dir_env(&backup_directory);

        let result = BackupService::backup_before_migration(&db_path).unwrap();
        clear_backup_dir_env();

        assert!(result.is_none());
    }

    #[test]
    #[serial]
    fn test_backup_existing_database() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");
        let backup_directory = temp_dir.path().join("backups");
        set_backup_dir_env(&backup_directory);

        // Create a fake database file
        std::fs::write(&db_path, b"test database content").unwrap();

        let result = BackupService::backup_before_migration(&db_path).unwrap();
        clear_backup_dir_env();

        assert!(result.is_some());

        let backup_path = result.unwrap();
        assert!(backup_path.exists());
        assert!(backup_path.to_string_lossy().contains("db_backup_"));
    }

    #[test]
    #[serial]
    fn test_cleanup_old_backups() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");
        let backup_directory = temp_dir.path().join("backups");
        std::fs::create_dir_all(&backup_directory).unwrap();
        set_backup_dir_env(&backup_directory);

        // Create 7 fake backup files
        for i in 0..7 {
            let backup_name = format!("db_backup_2025010{}_120000.sqlite", i);
            std::fs::write(backup_directory.join(&backup_name), format!("backup {}", i)).unwrap();
            // Small delay to ensure different modification times
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Cleanup keeping only 3
        BackupService::cleanup_old_backups_with_retention(&db_path, 3).unwrap();
        clear_backup_dir_env();

        // Count remaining backups
        let remaining: Vec<_> = std::fs::read_dir(&backup_directory)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();

        assert_eq!(remaining.len(), 3);
    }

    #[test]
    #[serial]
    fn test_list_backups_empty() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");
        let backup_directory = temp_dir.path().join("backups");
        // Don't create the directory - test empty case
        set_backup_dir_env(&backup_directory);

        // No backup directory exists yet
        let result = BackupService::list_backups(&db_path).unwrap();
        clear_backup_dir_env();

        assert!(result.is_empty());
    }

    #[test]
    #[serial]
    fn test_list_backups_sorted_newest_first() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");
        let backup_directory = temp_dir.path().join("backups");
        std::fs::create_dir_all(&backup_directory).unwrap();
        set_backup_dir_env(&backup_directory);

        // Create backup files with different timestamps
        std::fs::write(
            backup_directory.join("db_backup_20250101_100000.sqlite"),
            "old backup",
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(
            backup_directory.join("db_backup_20250102_100000.sqlite"),
            "new backup",
        )
        .unwrap();

        let result = BackupService::list_backups(&db_path).unwrap();
        clear_backup_dir_env();

        assert_eq!(result.len(), 2);
        // Newest first (sorted by modification time)
        assert!(result[0].filename.contains("20250102"));
        assert!(result[1].filename.contains("20250101"));
    }

    #[test]
    #[serial]
    fn test_create_backup() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");
        let backup_directory = temp_dir.path().join("backups");
        set_backup_dir_env(&backup_directory);

        // Create a fake database file
        std::fs::write(&db_path, b"test database content").unwrap();

        let info = BackupService::create_backup(&db_path).unwrap();
        clear_backup_dir_env();

        assert!(info.filename.starts_with("db_backup_"));
        assert!(info.filename.ends_with(".sqlite"));
        assert!(info.size_bytes > 0);

        // Verify the backup file exists
        assert!(backup_directory.join(&info.filename).exists());
    }

    #[test]
    #[serial]
    fn test_create_backup_nonexistent_database() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("nonexistent.sqlite");
        let backup_directory = temp_dir.path().join("backups");
        set_backup_dir_env(&backup_directory);

        let result = BackupService::create_backup(&db_path);
        clear_backup_dir_env();

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    #[serial]
    fn test_list_backups_ignores_non_backup_files() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");
        let backup_directory = temp_dir.path().join("backups");
        std::fs::create_dir_all(&backup_directory).unwrap();
        set_backup_dir_env(&backup_directory);

        // Create a valid backup file
        std::fs::write(
            backup_directory.join("db_backup_20250101_100000.sqlite"),
            "valid backup",
        )
        .unwrap();

        // Create files that should be ignored
        std::fs::write(backup_directory.join("random_file.sqlite"), "random").unwrap();
        std::fs::write(
            backup_directory.join("db_backup_20250101_100000.txt"),
            "wrong ext",
        )
        .unwrap();
        std::fs::write(backup_directory.join("other.db"), "other").unwrap();

        let result = BackupService::list_backups(&db_path).unwrap();
        clear_backup_dir_env();

        assert_eq!(result.len(), 1);
        assert!(result[0].filename.contains("db_backup_"));
    }

    #[test]
    #[serial]
    fn test_delete_backup() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");
        let backup_directory = temp_dir.path().join("backups");
        std::fs::create_dir_all(&backup_directory).unwrap();
        set_backup_dir_env(&backup_directory);

        // Create a backup file
        let filename = "db_backup_20250101_100000.sqlite";
        std::fs::write(backup_directory.join(filename), "backup content").unwrap();

        // Delete it
        BackupService::delete_backup(&db_path, filename).unwrap();
        clear_backup_dir_env();

        // Verify it's gone
        assert!(!backup_directory.join(filename).exists());
    }

    #[test]
    #[serial]
    fn test_delete_backup_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");
        let backup_directory = temp_dir.path().join("backups");
        std::fs::create_dir_all(&backup_directory).unwrap();
        set_backup_dir_env(&backup_directory);

        let result = BackupService::delete_backup(&db_path, "db_backup_20250101_100000.sqlite");
        clear_backup_dir_env();

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn test_delete_backup_rejects_path_traversal() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");

        // Test various path traversal attempts
        let malicious_filenames = [
            "../../../etc/passwd",
            "db_backup_../../../etc/passwd.sqlite",
            "..\\..\\windows\\system32\\config.sqlite",
            "db_backup_20250101_100000/../../etc/passwd.sqlite",
        ];

        for filename in malicious_filenames {
            let result = BackupService::delete_backup(&db_path, filename);
            assert!(result.is_err(), "Should reject: {}", filename);
            assert_eq!(
                result.unwrap_err().kind(),
                std::io::ErrorKind::InvalidInput,
                "Wrong error kind for: {}",
                filename
            );
        }
    }

    #[test]
    fn test_delete_backup_rejects_invalid_filenames() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");

        // Test filenames that don't match the expected pattern
        let invalid_filenames = [
            "not_a_backup.sqlite",
            "db_backup_20250101_100000.txt",
            "random_file.sqlite",
            "db_backup_.sqlite",
        ];

        for filename in invalid_filenames {
            let result = BackupService::delete_backup(&db_path, filename);
            assert!(result.is_err(), "Should reject: {}", filename);
        }
    }

    #[test]
    #[serial]
    fn test_get_backup_path() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");
        let backup_directory = temp_dir.path().join("backups");
        std::fs::create_dir_all(&backup_directory).unwrap();
        set_backup_dir_env(&backup_directory);

        // Create a backup file
        let filename = "db_backup_20250101_100000.sqlite";
        std::fs::write(backup_directory.join(filename), "backup content").unwrap();

        // Get the path
        let result = BackupService::get_backup_path(&db_path, filename).unwrap();
        clear_backup_dir_env();

        assert_eq!(result, backup_directory.join(filename));
        assert!(result.exists());
    }

    #[test]
    #[serial]
    fn test_get_backup_path_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");
        let backup_directory = temp_dir.path().join("backups");
        std::fs::create_dir_all(&backup_directory).unwrap();
        set_backup_dir_env(&backup_directory);

        let result = BackupService::get_backup_path(&db_path, "db_backup_20250101_100000.sqlite");
        clear_backup_dir_env();

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn test_get_backup_path_rejects_path_traversal() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");

        // Test various path traversal attempts
        let malicious_filenames = [
            "../../../etc/passwd",
            "db_backup_../../../etc/passwd.sqlite",
            "..\\..\\windows\\system32\\config.sqlite",
            "db_backup_20250101_100000/../../etc/passwd.sqlite",
        ];

        for filename in malicious_filenames {
            let result = BackupService::get_backup_path(&db_path, filename);
            assert!(result.is_err(), "Should reject: {}", filename);
            assert_eq!(
                result.unwrap_err().kind(),
                std::io::ErrorKind::InvalidInput,
                "Wrong error kind for: {}",
                filename
            );
        }
    }

    #[test]
    fn test_get_backup_path_rejects_invalid_filenames() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");

        // Test filenames that don't match the expected pattern
        let invalid_filenames = [
            "not_a_backup.sqlite",
            "db_backup_20250101_100000.txt",
            "random_file.sqlite",
            "db_backup_.sqlite",
        ];

        for filename in invalid_filenames {
            let result = BackupService::get_backup_path(&db_path, filename);
            assert!(result.is_err(), "Should reject: {}", filename);
        }
    }

    #[test]
    #[serial]
    fn test_restore_from_data_valid_sqlite() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");
        let backup_directory = temp_dir.path().join("backups");
        set_backup_dir_env(&backup_directory);

        // Create a fake SQLite file with valid header
        let mut data = b"SQLite format 3\0".to_vec();
        data.extend_from_slice(&[0u8; 100]); // Pad with zeros

        // Restore it
        BackupService::restore_from_data(&db_path, &data).unwrap();
        clear_backup_dir_env();

        // Verify the file was created
        assert!(db_path.exists());
        let content = std::fs::read(&db_path).unwrap();
        assert_eq!(&content[0..16], b"SQLite format 3\0");
    }

    #[test]
    #[serial]
    fn test_restore_from_data_rejects_invalid() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");
        let backup_directory = temp_dir.path().join("backups");
        set_backup_dir_env(&backup_directory);

        // Try to restore invalid data
        let result = BackupService::restore_from_data(&db_path, b"not sqlite data");
        clear_backup_dir_env();

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    #[serial]
    fn test_restore_from_data_rejects_empty() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");
        let backup_directory = temp_dir.path().join("backups");
        set_backup_dir_env(&backup_directory);

        // Try to restore empty data
        let result = BackupService::restore_from_data(&db_path, &[]);
        clear_backup_dir_env();

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    #[serial]
    fn test_restore_from_data_creates_backup_first() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");
        let backup_directory = temp_dir.path().join("backups");
        set_backup_dir_env(&backup_directory);

        // Create an existing database
        let mut existing_data = b"SQLite format 3\0".to_vec();
        existing_data.extend_from_slice(b"existing content");
        std::fs::write(&db_path, &existing_data).unwrap();

        // Create new data to restore
        let mut new_data = b"SQLite format 3\0".to_vec();
        new_data.extend_from_slice(b"new content here");

        // Restore it
        BackupService::restore_from_data(&db_path, &new_data).unwrap();
        clear_backup_dir_env();

        // Verify backup was created
        assert!(backup_directory.exists());
        let backup_count = std::fs::read_dir(&backup_directory)
            .unwrap()
            .filter(|e| e.is_ok())
            .count();
        assert!(backup_count >= 1, "Backup should have been created");

        // Verify the new content was written
        let content = std::fs::read(&db_path).unwrap();
        assert_eq!(content, new_data);
    }

    #[test]
    #[serial]
    fn test_restore_from_data_removes_wal_shm() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");
        let wal_path = db_path.with_extension("sqlite-wal");
        let shm_path = db_path.with_extension("sqlite-shm");
        let backup_directory = temp_dir.path().join("backups");
        set_backup_dir_env(&backup_directory);

        // Create existing database with WAL/SHM files
        let mut existing_data = b"SQLite format 3\0".to_vec();
        existing_data.extend_from_slice(&[0u8; 100]);
        std::fs::write(&db_path, &existing_data).unwrap();
        std::fs::write(&wal_path, b"wal content").unwrap();
        std::fs::write(&shm_path, b"shm content").unwrap();

        // Create new data to restore
        let mut new_data = b"SQLite format 3\0".to_vec();
        new_data.extend_from_slice(&[0u8; 100]);

        // Restore it
        BackupService::restore_from_data(&db_path, &new_data).unwrap();
        clear_backup_dir_env();

        // Verify WAL/SHM were removed
        assert!(!wal_path.exists(), "WAL file should be removed");
        assert!(!shm_path.exists(), "SHM file should be removed");
    }
}
