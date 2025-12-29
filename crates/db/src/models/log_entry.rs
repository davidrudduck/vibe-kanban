//! Log entry model for ElectricSQL-compatible log storage.
//!
//! This module provides a database model for storing log entries as individual rows,
//! enabling row-level sync with ElectricSQL's shape subscriptions. Unlike the older
//! `execution_process_logs` table which stores JSONL batches, this table stores
//! one row per log message for efficient pagination and real-time sync.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use utils::unified_log::{Direction, OutputType, PaginatedLogs};
use uuid::Uuid;

/// A single log entry stored in the database.
///
/// This struct maps to the `log_entries` table and stores individual log messages
/// rather than batched JSONL. This design is required for ElectricSQL compatibility.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DbLogEntry {
    /// Auto-incrementing primary key for cursor-based pagination.
    pub id: i64,
    /// The execution process this log belongs to.
    pub execution_id: Uuid,
    /// The type of output (stdout, stderr, system, etc.).
    pub output_type: String,
    /// The content of the log message.
    pub content: String,
    /// When this log entry was created.
    #[ts(type = "string")]
    pub timestamp: DateTime<Utc>,
    /// When this log entry was synced to the Hive. NULL means not yet synced.
    #[ts(optional, type = "string")]
    pub hive_synced_at: Option<DateTime<Utc>>,
}

/// Request struct for creating a new log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateLogEntry {
    pub execution_id: Uuid,
    pub output_type: String,
    pub content: String,
}

impl CreateLogEntry {
    /// Create a new log entry request with validated output type.
    pub fn new(execution_id: Uuid, output_type: OutputType, content: String) -> Self {
        Self {
            execution_id,
            output_type: output_type.as_str().to_string(),
            content,
        }
    }

    /// Create a stdout log entry.
    pub fn stdout(execution_id: Uuid, content: String) -> Self {
        Self::new(execution_id, OutputType::Stdout, content)
    }

    /// Create a stderr log entry.
    pub fn stderr(execution_id: Uuid, content: String) -> Self {
        Self::new(execution_id, OutputType::Stderr, content)
    }
}

/// Paginated result for log entry queries.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PaginatedDbLogEntries {
    /// The log entries for this page.
    pub entries: Vec<DbLogEntry>,
    /// Cursor for the next page (if more entries exist).
    pub next_cursor: Option<i64>,
    /// Whether there are more entries after this page.
    pub has_more: bool,
    /// Total count of log entries (if available).
    pub total_count: Option<i64>,
}

impl PaginatedDbLogEntries {
    /// Create an empty result.
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            next_cursor: None,
            has_more: false,
            total_count: Some(0),
        }
    }

    /// Convert to the unified PaginatedLogs format for API responses.
    pub fn to_paginated_logs(self) -> PaginatedLogs {
        let entries = self
            .entries
            .into_iter()
            .map(|e| {
                utils::unified_log::LogEntry::new(
                    e.id,
                    e.content,
                    OutputType::from_remote_str(&e.output_type),
                    e.timestamp,
                    e.execution_id,
                )
            })
            .collect();

        PaginatedLogs::new(entries, self.next_cursor, self.has_more, self.total_count)
    }
}

impl DbLogEntry {
    /// Create a new log entry in the database.
    pub async fn create(pool: &SqlitePool, data: CreateLogEntry) -> Result<Self, sqlx::Error> {
        let row = sqlx::query_as!(
            DbLogEntry,
            r#"INSERT INTO log_entries (execution_id, output_type, content, timestamp)
               VALUES ($1, $2, $3, datetime('now', 'subsec'))
               RETURNING
                   id as "id!",
                   execution_id as "execution_id!: Uuid",
                   output_type,
                   content,
                   timestamp as "timestamp!: DateTime<Utc>",
                   hive_synced_at as "hive_synced_at: DateTime<Utc>""#,
            data.execution_id,
            data.output_type,
            data.content
        )
        .fetch_one(pool)
        .await?;

        Ok(row)
    }

    /// Find a log entry by ID.
    pub async fn find_by_id(pool: &SqlitePool, id: i64) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            DbLogEntry,
            r#"SELECT
                id as "id!",
                execution_id as "execution_id!: Uuid",
                output_type,
                content,
                timestamp as "timestamp!: DateTime<Utc>",
                hive_synced_at as "hive_synced_at: DateTime<Utc>"
               FROM log_entries
               WHERE id = $1"#,
            id
        )
        .fetch_optional(pool)
        .await
    }

    /// Find all log entries for an execution process.
    pub async fn find_by_execution_id(
        pool: &SqlitePool,
        execution_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            DbLogEntry,
            r#"SELECT
                id as "id!",
                execution_id as "execution_id!: Uuid",
                output_type,
                content,
                timestamp as "timestamp!: DateTime<Utc>",
                hive_synced_at as "hive_synced_at: DateTime<Utc>"
               FROM log_entries
               WHERE execution_id = $1
               ORDER BY id ASC"#,
            execution_id
        )
        .fetch_all(pool)
        .await
    }

    /// Find paginated log entries for an execution process.
    ///
    /// # Arguments
    /// * `pool` - Database connection pool
    /// * `execution_id` - The execution process ID to fetch logs for
    /// * `cursor` - Optional cursor (entry ID) to start from
    /// * `limit` - Maximum number of entries to return
    /// * `direction` - Forward (oldest first) or Backward (newest first)
    ///
    /// # Returns
    /// A `PaginatedDbLogEntries` struct containing the entries and pagination info.
    pub async fn find_paginated(
        pool: &SqlitePool,
        execution_id: Uuid,
        cursor: Option<i64>,
        limit: i64,
        direction: Direction,
    ) -> Result<PaginatedDbLogEntries, sqlx::Error> {
        // Get total count first
        let total_count: i64 = sqlx::query_scalar!(
            r#"SELECT COUNT(*) as "count!: i64" FROM log_entries WHERE execution_id = $1"#,
            execution_id
        )
        .fetch_one(pool)
        .await?;

        if total_count == 0 {
            return Ok(PaginatedDbLogEntries::empty());
        }

        // Fetch one extra to determine has_more
        let fetch_limit = limit + 1;

        let entries = match direction {
            Direction::Forward => {
                if let Some(cursor_id) = cursor {
                    sqlx::query_as!(
                        DbLogEntry,
                        r#"SELECT
                            id as "id!",
                            execution_id as "execution_id!: Uuid",
                            output_type,
                            content,
                            timestamp as "timestamp!: DateTime<Utc>",
                            hive_synced_at as "hive_synced_at: DateTime<Utc>"
                           FROM log_entries
                           WHERE execution_id = $1 AND id > $2
                           ORDER BY id ASC
                           LIMIT $3"#,
                        execution_id,
                        cursor_id,
                        fetch_limit
                    )
                    .fetch_all(pool)
                    .await?
                } else {
                    sqlx::query_as!(
                        DbLogEntry,
                        r#"SELECT
                            id as "id!",
                            execution_id as "execution_id!: Uuid",
                            output_type,
                            content,
                            timestamp as "timestamp!: DateTime<Utc>",
                            hive_synced_at as "hive_synced_at: DateTime<Utc>"
                           FROM log_entries
                           WHERE execution_id = $1
                           ORDER BY id ASC
                           LIMIT $2"#,
                        execution_id,
                        fetch_limit
                    )
                    .fetch_all(pool)
                    .await?
                }
            }
            Direction::Backward => {
                if let Some(cursor_id) = cursor {
                    sqlx::query_as!(
                        DbLogEntry,
                        r#"SELECT
                            id as "id!",
                            execution_id as "execution_id!: Uuid",
                            output_type,
                            content,
                            timestamp as "timestamp!: DateTime<Utc>",
                            hive_synced_at as "hive_synced_at: DateTime<Utc>"
                           FROM log_entries
                           WHERE execution_id = $1 AND id < $2
                           ORDER BY id DESC
                           LIMIT $3"#,
                        execution_id,
                        cursor_id,
                        fetch_limit
                    )
                    .fetch_all(pool)
                    .await?
                } else {
                    sqlx::query_as!(
                        DbLogEntry,
                        r#"SELECT
                            id as "id!",
                            execution_id as "execution_id!: Uuid",
                            output_type,
                            content,
                            timestamp as "timestamp!: DateTime<Utc>",
                            hive_synced_at as "hive_synced_at: DateTime<Utc>"
                           FROM log_entries
                           WHERE execution_id = $1
                           ORDER BY id DESC
                           LIMIT $2"#,
                        execution_id,
                        fetch_limit
                    )
                    .fetch_all(pool)
                    .await?
                }
            }
        };

        let has_more = entries.len() > limit as usize;
        let entries: Vec<DbLogEntry> = entries.into_iter().take(limit as usize).collect();

        let next_cursor = if has_more {
            entries.last().map(|e| e.id)
        } else {
            None
        };

        Ok(PaginatedDbLogEntries {
            entries,
            next_cursor,
            has_more,
            total_count: Some(total_count),
        })
    }

    /// Delete all log entries for an execution process.
    pub async fn delete_by_execution_id(
        pool: &SqlitePool,
        execution_id: Uuid,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            "DELETE FROM log_entries WHERE execution_id = $1",
            execution_id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Find log entries that have not been synced to the Hive.
    /// Returns entries grouped by execution_id and ordered by id (oldest first).
    /// This allows batching log entries for efficient sync.
    pub async fn find_unsynced(pool: &SqlitePool, limit: i64) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            DbLogEntry,
            r#"SELECT
                id as "id!",
                execution_id as "execution_id!: Uuid",
                output_type,
                content,
                timestamp as "timestamp!: DateTime<Utc>",
                hive_synced_at as "hive_synced_at: DateTime<Utc>"
               FROM log_entries
               WHERE hive_synced_at IS NULL
               ORDER BY execution_id, id ASC
               LIMIT $1"#,
            limit
        )
        .fetch_all(pool)
        .await
    }

    /// Mark a log entry as synced to the Hive.
    pub async fn mark_hive_synced(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
        let now = Utc::now();
        sqlx::query!(
            "UPDATE log_entries SET hive_synced_at = $1 WHERE id = $2",
            now,
            id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Mark multiple log entries as synced to the Hive.
    pub async fn mark_hive_synced_batch(
        pool: &SqlitePool,
        ids: &[i64],
    ) -> Result<u64, sqlx::Error> {
        if ids.is_empty() {
            return Ok(0);
        }

        let now = Utc::now();
        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("${}", i + 1)).collect();
        let query = format!(
            "UPDATE log_entries SET hive_synced_at = $1 WHERE id IN ({})",
            placeholders.join(", ")
        );

        let mut query_builder = sqlx::query(&query).bind(now);
        for id in ids {
            query_builder = query_builder.bind(id);
        }

        let result = query_builder.execute(pool).await?;
        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
    use std::str::FromStr;
    use tempfile::TempDir;

    /// Create a test SQLite pool with migrations applied.
    async fn setup_test_pool() -> (SqlitePool, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join("test.db");

        let options =
            SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.to_string_lossy()))
                .expect("Invalid database URL")
                .create_if_missing(true)
                .journal_mode(SqliteJournalMode::Wal);

        let pool = SqlitePool::connect_with(options)
            .await
            .expect("Failed to create pool");

        // Run migrations
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        (pool, temp_dir)
    }

    /// Create a test execution process for log entry tests.
    /// Returns the execution_id.
    async fn create_test_execution(pool: &SqlitePool) -> Uuid {
        use crate::models::{
            project::{CreateProject, Project},
            task::{CreateTask, Task},
        };

        // Create project with unique path
        let project_id = Uuid::new_v4();
        let project_data = CreateProject {
            name: "Test Project".to_string(),
            git_repo_path: format!("/tmp/test-repo-{}", project_id),
            use_existing_repo: true,
            clone_url: None,
            setup_script: None,
            dev_script: None,
            cleanup_script: None,
            copy_files: None,
        };
        let _project = Project::create(pool, &project_data, project_id)
            .await
            .expect("Failed to create project");

        // Create task
        let task_id = Uuid::new_v4();
        let task_data =
            CreateTask::from_title_description(project_id, "Test Task".to_string(), None);
        let _task = Task::create(pool, &task_data, task_id)
            .await
            .expect("Failed to create task");

        // Create task attempt using raw SQL to avoid complex dependencies
        let attempt_id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO task_attempts (id, task_id, executor, branch, target_branch)
               VALUES ($1, $2, 'CLAUDE_CODE', 'test-branch', 'main')"#,
        )
        .bind(attempt_id)
        .bind(task_id)
        .execute(pool)
        .await
        .expect("Failed to create task attempt");

        // Create execution process (executor_action needs valid JSON for the virtual column)
        let execution_id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO execution_processes (id, task_attempt_id, status, run_reason, executor_action)
               VALUES ($1, $2, 'running', 'codingagent', '{}')"#,
        )
        .bind(execution_id)
        .bind(attempt_id)
        .execute(pool)
        .await
        .expect("Failed to create execution process");

        execution_id
    }

    #[tokio::test]
    async fn test_log_entry_create() {
        let (pool, _temp_dir) = setup_test_pool().await;
        let execution_id = create_test_execution(&pool).await;

        let entry = DbLogEntry::create(
            &pool,
            CreateLogEntry {
                execution_id,
                output_type: "stdout".into(),
                content: "Hello, world!".into(),
            },
        )
        .await
        .expect("Failed to create log entry");

        assert_eq!(entry.content, "Hello, world!");
        assert_eq!(entry.output_type, "stdout");
        assert_eq!(entry.execution_id, execution_id);
        assert!(entry.id > 0);
    }

    #[tokio::test]
    async fn test_log_entry_find_by_id() {
        let (pool, _temp_dir) = setup_test_pool().await;
        let execution_id = create_test_execution(&pool).await;

        let created = DbLogEntry::create(
            &pool,
            CreateLogEntry::stdout(execution_id, "Test content".into()),
        )
        .await
        .expect("Failed to create log entry");

        let found = DbLogEntry::find_by_id(&pool, created.id)
            .await
            .expect("Query failed")
            .expect("Log entry not found");

        assert_eq!(found.id, created.id);
        assert_eq!(found.content, "Test content");
    }

    #[tokio::test]
    async fn test_log_entry_find_by_execution_id() {
        let (pool, _temp_dir) = setup_test_pool().await;
        let execution_id = create_test_execution(&pool).await;

        // Create multiple entries
        for i in 0..5 {
            DbLogEntry::create(
                &pool,
                CreateLogEntry::stdout(execution_id, format!("Message {}", i)),
            )
            .await
            .expect("Failed to create log entry");
        }

        let entries = DbLogEntry::find_by_execution_id(&pool, execution_id)
            .await
            .expect("Query failed");

        assert_eq!(entries.len(), 5);
        // Verify order (ascending by id)
        for (i, entry) in entries.iter().enumerate().take(5) {
            assert_eq!(entry.content, format!("Message {}", i));
        }
    }

    #[tokio::test]
    async fn test_log_entry_pagination_forward_no_cursor() {
        let (pool, _temp_dir) = setup_test_pool().await;
        let execution_id = create_test_execution(&pool).await;

        // Create 10 entries
        for i in 0..10 {
            DbLogEntry::create(
                &pool,
                CreateLogEntry::stdout(execution_id, format!("Line {}", i)),
            )
            .await
            .expect("Failed to create log entry");
        }

        // Paginate: first 3
        let page1 = DbLogEntry::find_paginated(&pool, execution_id, None, 3, Direction::Forward)
            .await
            .expect("Query failed");

        assert_eq!(page1.entries.len(), 3);
        assert!(page1.has_more);
        assert!(page1.next_cursor.is_some());
        assert_eq!(page1.total_count, Some(10));
        assert_eq!(page1.entries[0].content, "Line 0");
        assert_eq!(page1.entries[2].content, "Line 2");
    }

    #[tokio::test]
    async fn test_log_entry_pagination_forward_with_cursor() {
        let (pool, _temp_dir) = setup_test_pool().await;
        let execution_id = create_test_execution(&pool).await;

        // Create 10 entries
        for i in 0..10 {
            DbLogEntry::create(
                &pool,
                CreateLogEntry::stdout(execution_id, format!("Line {}", i)),
            )
            .await
            .expect("Failed to create log entry");
        }

        // Get first page
        let page1 = DbLogEntry::find_paginated(&pool, execution_id, None, 3, Direction::Forward)
            .await
            .expect("Query failed");

        // Get second page using cursor
        let page2 = DbLogEntry::find_paginated(
            &pool,
            execution_id,
            page1.next_cursor,
            3,
            Direction::Forward,
        )
        .await
        .expect("Query failed");

        assert_eq!(page2.entries.len(), 3);
        assert!(page2.has_more);
        assert_eq!(page2.entries[0].content, "Line 3");
        assert_eq!(page2.entries[2].content, "Line 5");
    }

    #[tokio::test]
    async fn test_log_entry_pagination_forward_last_page() {
        let (pool, _temp_dir) = setup_test_pool().await;
        let execution_id = create_test_execution(&pool).await;

        // Create 5 entries
        for i in 0..5 {
            DbLogEntry::create(
                &pool,
                CreateLogEntry::stdout(execution_id, format!("Line {}", i)),
            )
            .await
            .expect("Failed to create log entry");
        }

        // Get first page
        let page1 = DbLogEntry::find_paginated(&pool, execution_id, None, 3, Direction::Forward)
            .await
            .expect("Query failed");

        // Get second page (should be last)
        let page2 = DbLogEntry::find_paginated(
            &pool,
            execution_id,
            page1.next_cursor,
            3,
            Direction::Forward,
        )
        .await
        .expect("Query failed");

        assert_eq!(page2.entries.len(), 2); // Only 2 remaining
        assert!(!page2.has_more);
        assert!(page2.next_cursor.is_none());
    }

    #[tokio::test]
    async fn test_log_entry_pagination_backward_no_cursor() {
        let (pool, _temp_dir) = setup_test_pool().await;
        let execution_id = create_test_execution(&pool).await;

        // Create 10 entries
        for i in 0..10 {
            DbLogEntry::create(
                &pool,
                CreateLogEntry::stdout(execution_id, format!("Line {}", i)),
            )
            .await
            .expect("Failed to create log entry");
        }

        // Paginate backward (newest first)
        let page1 = DbLogEntry::find_paginated(&pool, execution_id, None, 3, Direction::Backward)
            .await
            .expect("Query failed");

        assert_eq!(page1.entries.len(), 3);
        assert!(page1.has_more);
        // Backward returns newest first
        assert_eq!(page1.entries[0].content, "Line 9");
        assert_eq!(page1.entries[2].content, "Line 7");
    }

    #[tokio::test]
    async fn test_log_entry_pagination_backward_with_cursor() {
        let (pool, _temp_dir) = setup_test_pool().await;
        let execution_id = create_test_execution(&pool).await;

        // Create 10 entries
        for i in 0..10 {
            DbLogEntry::create(
                &pool,
                CreateLogEntry::stdout(execution_id, format!("Line {}", i)),
            )
            .await
            .expect("Failed to create log entry");
        }

        // Get first page (newest)
        let page1 = DbLogEntry::find_paginated(&pool, execution_id, None, 3, Direction::Backward)
            .await
            .expect("Query failed");

        // Get second page using cursor
        let page2 = DbLogEntry::find_paginated(
            &pool,
            execution_id,
            page1.next_cursor,
            3,
            Direction::Backward,
        )
        .await
        .expect("Query failed");

        assert_eq!(page2.entries.len(), 3);
        // Should continue from where we left off
        assert_eq!(page2.entries[0].content, "Line 6");
        assert_eq!(page2.entries[2].content, "Line 4");
    }

    #[tokio::test]
    async fn test_log_entry_pagination_empty() {
        let (pool, _temp_dir) = setup_test_pool().await;
        let execution_id = create_test_execution(&pool).await;

        // No entries created
        let result = DbLogEntry::find_paginated(&pool, execution_id, None, 10, Direction::Forward)
            .await
            .expect("Query failed");

        assert!(result.entries.is_empty());
        assert!(!result.has_more);
        assert!(result.next_cursor.is_none());
        assert_eq!(result.total_count, Some(0));
    }

    #[tokio::test]
    async fn test_log_entry_delete_by_execution_id() {
        let (pool, _temp_dir) = setup_test_pool().await;
        let execution_id = create_test_execution(&pool).await;

        // Create entries
        for i in 0..5 {
            DbLogEntry::create(
                &pool,
                CreateLogEntry::stdout(execution_id, format!("Line {}", i)),
            )
            .await
            .expect("Failed to create log entry");
        }

        // Delete all entries
        let deleted = DbLogEntry::delete_by_execution_id(&pool, execution_id)
            .await
            .expect("Delete failed");

        assert_eq!(deleted, 5);

        // Verify entries are gone
        let remaining = DbLogEntry::find_by_execution_id(&pool, execution_id)
            .await
            .expect("Query failed");

        assert!(remaining.is_empty());
    }

    #[tokio::test]
    async fn test_log_entry_output_types() {
        let (pool, _temp_dir) = setup_test_pool().await;
        let execution_id = create_test_execution(&pool).await;

        // Create entries with different output types
        DbLogEntry::create(
            &pool,
            CreateLogEntry::stdout(execution_id, "stdout msg".into()),
        )
        .await
        .expect("Failed to create stdout entry");

        DbLogEntry::create(
            &pool,
            CreateLogEntry::stderr(execution_id, "stderr msg".into()),
        )
        .await
        .expect("Failed to create stderr entry");

        DbLogEntry::create(
            &pool,
            CreateLogEntry::new(execution_id, OutputType::System, "system msg".into()),
        )
        .await
        .expect("Failed to create system entry");

        let entries = DbLogEntry::find_by_execution_id(&pool, execution_id)
            .await
            .expect("Query failed");

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].output_type, "stdout");
        assert_eq!(entries[1].output_type, "stderr");
        assert_eq!(entries[2].output_type, "system");
    }

    #[tokio::test]
    async fn test_log_entry_to_paginated_logs() {
        let (pool, _temp_dir) = setup_test_pool().await;
        let execution_id = create_test_execution(&pool).await;

        // Create entries
        DbLogEntry::create(&pool, CreateLogEntry::stdout(execution_id, "Hello".into()))
            .await
            .expect("Failed to create log entry");

        let db_result =
            DbLogEntry::find_paginated(&pool, execution_id, None, 10, Direction::Forward)
                .await
                .expect("Query failed");

        // Convert to unified format
        let paginated_logs = db_result.to_paginated_logs();

        assert_eq!(paginated_logs.entries.len(), 1);
        assert_eq!(paginated_logs.entries[0].content, "Hello");
        assert_eq!(paginated_logs.entries[0].output_type, OutputType::Stdout);
        assert_eq!(paginated_logs.entries[0].execution_id, execution_id);
    }

    #[tokio::test]
    async fn test_log_entry_isolation_between_executions() {
        let (pool, _temp_dir) = setup_test_pool().await;
        let execution_id_1 = create_test_execution(&pool).await;
        let execution_id_2 = create_test_execution(&pool).await;

        // Create entries for execution 1
        for i in 0..3 {
            DbLogEntry::create(
                &pool,
                CreateLogEntry::stdout(execution_id_1, format!("Exec1 Line {}", i)),
            )
            .await
            .expect("Failed to create log entry");
        }

        // Create entries for execution 2
        for i in 0..2 {
            DbLogEntry::create(
                &pool,
                CreateLogEntry::stdout(execution_id_2, format!("Exec2 Line {}", i)),
            )
            .await
            .expect("Failed to create log entry");
        }

        // Verify isolation
        let entries_1 = DbLogEntry::find_by_execution_id(&pool, execution_id_1)
            .await
            .expect("Query failed");
        let entries_2 = DbLogEntry::find_by_execution_id(&pool, execution_id_2)
            .await
            .expect("Query failed");

        assert_eq!(entries_1.len(), 3);
        assert_eq!(entries_2.len(), 2);

        // All entries in entries_1 should belong to execution_id_1
        for entry in &entries_1 {
            assert_eq!(entry.execution_id, execution_id_1);
            assert!(entry.content.starts_with("Exec1"));
        }

        // All entries in entries_2 should belong to execution_id_2
        for entry in &entries_2 {
            assert_eq!(entry.execution_id, execution_id_2);
            assert!(entry.content.starts_with("Exec2"));
        }
    }
}
