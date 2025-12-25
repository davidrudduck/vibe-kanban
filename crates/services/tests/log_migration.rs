//! Integration tests for JSONL to log_entries migration.
//!
//! These tests verify that the log migration service correctly:
//! 1. Reads JSONL log records from execution_process_logs table
//! 2. Parses each JSONL line into individual log entries
//! 3. Inserts entries into the new log_entries table
//! 4. Handles various log types (stdout, stderr, json_patch, etc.)
//! 5. Preserves execution_id and timestamp associations
//! 6. Supports dry-run and execute modes

use db::models::log_entry::DbLogEntry;
use sqlx::{Row, SqlitePool};
use std::str::FromStr;
use tempfile::TempDir;
use uuid::Uuid;

/// Create a test database with migrations applied.
async fn setup_test_db() -> (SqlitePool, TempDir) {
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};

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
    sqlx::migrate!("../db/migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    (pool, temp_dir)
}

/// Create a test execution process and return its ID.
async fn create_test_execution(pool: &SqlitePool) -> Uuid {
    // Create project with unique path
    let project_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO projects (id, name, git_repo_path)
           VALUES ($1, 'Test Project', $2)"#,
    )
    .bind(project_id)
    .bind(format!("/tmp/test-{}", project_id))
    .execute(pool)
    .await
    .expect("Failed to create project");

    // Create task
    let task_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO tasks (id, project_id, title)
           VALUES ($1, $2, 'Test Task')"#,
    )
    .bind(task_id)
    .bind(project_id)
    .execute(pool)
    .await
    .expect("Failed to create task");

    // Create task attempt
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

    // Create execution process
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

/// Insert JSONL log records into the old execution_process_logs table.
async fn insert_jsonl_logs(pool: &SqlitePool, execution_id: Uuid, jsonl: &str) {
    let byte_size = jsonl.len() as i64;
    sqlx::query(
        r#"INSERT INTO execution_process_logs (execution_id, logs, byte_size, inserted_at)
           VALUES ($1, $2, $3, datetime('now', 'subsec'))"#,
    )
    .bind(execution_id)
    .bind(jsonl)
    .bind(byte_size)
    .execute(pool)
    .await
    .expect("Failed to insert JSONL log");
}

/// Count log entries in the new log_entries table.
async fn count_log_entries(pool: &SqlitePool, execution_id: Uuid) -> i64 {
    let row = sqlx::query(
        r#"SELECT COUNT(*) as count FROM log_entries WHERE execution_id = $1"#,
    )
    .bind(execution_id)
    .fetch_one(pool)
    .await
    .expect("Failed to count log entries");

    row.get::<i64, _>("count")
}

/// Get all log entries for an execution.
async fn get_log_entries(pool: &SqlitePool, execution_id: Uuid) -> Vec<DbLogEntry> {
    DbLogEntry::find_by_execution_id(pool, execution_id)
        .await
        .expect("Failed to fetch log entries")
}

// =============================================================================
// TESTS
// =============================================================================

#[tokio::test]
async fn test_migrate_single_stdout_log() {
    let (pool, _temp_dir) = setup_test_db().await;
    let execution_id = create_test_execution(&pool).await;

    // Insert a single JSONL line
    let jsonl = r#"{"Stdout":"Hello, world!"}"#;
    insert_jsonl_logs(&pool, execution_id, jsonl).await;

    // Migrate logs
    let result = services::services::log_migration::migrate_execution_logs(&pool, execution_id)
        .await
        .expect("Migration failed");

    // Verify result
    assert_eq!(result.migrated, 1);
    assert_eq!(result.skipped, 0);
    assert_eq!(result.errors, 0);

    // Verify log entry in database
    let entries = get_log_entries(&pool, execution_id).await;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].content, "Hello, world!");
    assert_eq!(entries[0].output_type, "stdout");
}

#[tokio::test]
async fn test_migrate_multiple_log_lines() {
    let (pool, _temp_dir) = setup_test_db().await;
    let execution_id = create_test_execution(&pool).await;

    // Insert multiple JSONL lines
    let jsonl = r#"{"Stdout":"Line 1"}
{"Stdout":"Line 2"}
{"Stderr":"Error message"}
{"Stdout":"Line 3"}"#;
    insert_jsonl_logs(&pool, execution_id, jsonl).await;

    // Migrate logs
    let result = services::services::log_migration::migrate_execution_logs(&pool, execution_id)
        .await
        .expect("Migration failed");

    // Verify result
    assert_eq!(result.migrated, 4);

    // Verify log entries
    let entries = get_log_entries(&pool, execution_id).await;
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0].content, "Line 1");
    assert_eq!(entries[1].content, "Line 2");
    assert_eq!(entries[2].content, "Error message");
    assert_eq!(entries[2].output_type, "stderr");
    assert_eq!(entries[3].content, "Line 3");
}

#[tokio::test]
async fn test_migrate_all_log_types() {
    let (pool, _temp_dir) = setup_test_db().await;
    let execution_id = create_test_execution(&pool).await;

    // Insert all log types
    let jsonl = r#"{"Stdout":"stdout message"}
{"Stderr":"stderr message"}
{"SessionId":"session123"}
"Finished"
{"RefreshRequired":{"reason":"reconnect needed"}}
{"JsonPatch":[{"op":"add","path":"/foo","value":"bar"}]}"#;
    insert_jsonl_logs(&pool, execution_id, jsonl).await;

    // Migrate logs
    let result = services::services::log_migration::migrate_execution_logs(&pool, execution_id)
        .await
        .expect("Migration failed");

    assert_eq!(result.migrated, 6);

    // Verify log entries
    let entries = get_log_entries(&pool, execution_id).await;
    assert_eq!(entries.len(), 6);

    assert_eq!(entries[0].output_type, "stdout");
    assert_eq!(entries[0].content, "stdout message");

    assert_eq!(entries[1].output_type, "stderr");
    assert_eq!(entries[1].content, "stderr message");

    assert_eq!(entries[2].output_type, "session_id");
    assert_eq!(entries[2].content, "session123");

    assert_eq!(entries[3].output_type, "finished");
    assert!(entries[3].content.is_empty());

    assert_eq!(entries[4].output_type, "refresh_required");
    assert_eq!(entries[4].content, "reconnect needed");

    assert_eq!(entries[5].output_type, "json_patch");
    assert!(entries[5].content.contains("add"));
}

#[tokio::test]
async fn test_migrate_skips_empty_lines() {
    let (pool, _temp_dir) = setup_test_db().await;
    let execution_id = create_test_execution(&pool).await;

    // Insert JSONL with empty lines
    let jsonl = r#"{"Stdout":"Line 1"}

{"Stdout":"Line 2"}

{"Stdout":"Line 3"}"#;
    insert_jsonl_logs(&pool, execution_id, jsonl).await;

    let result = services::services::log_migration::migrate_execution_logs(&pool, execution_id)
        .await
        .expect("Migration failed");

    assert_eq!(result.migrated, 3);
    assert_eq!(count_log_entries(&pool, execution_id).await, 3);
}

#[tokio::test]
async fn test_migrate_handles_invalid_json() {
    let (pool, _temp_dir) = setup_test_db().await;
    let execution_id = create_test_execution(&pool).await;

    // Insert JSONL with some invalid lines
    let jsonl = r#"{"Stdout":"Valid line 1"}
not valid json
{"Stdout":"Valid line 2"}
{"Invalid":"type"}
{"Stdout":"Valid line 3"}"#;
    insert_jsonl_logs(&pool, execution_id, jsonl).await;

    let result = services::services::log_migration::migrate_execution_logs(&pool, execution_id)
        .await
        .expect("Migration failed");

    // 3 valid lines migrated, 2 with errors
    assert_eq!(result.migrated, 3);
    assert_eq!(result.errors, 2);

    let entries = get_log_entries(&pool, execution_id).await;
    assert_eq!(entries.len(), 3);
}

#[tokio::test]
async fn test_migrate_multiple_records() {
    let (pool, _temp_dir) = setup_test_db().await;
    let execution_id = create_test_execution(&pool).await;

    // Insert multiple JSONL records (simulating batch inserts)
    insert_jsonl_logs(&pool, execution_id, r#"{"Stdout":"Record 1 Line 1"}"#).await;
    insert_jsonl_logs(&pool, execution_id, r#"{"Stdout":"Record 2 Line 1"}
{"Stdout":"Record 2 Line 2"}"#).await;
    insert_jsonl_logs(&pool, execution_id, r#"{"Stderr":"Record 3 Error"}"#).await;

    let result = services::services::log_migration::migrate_execution_logs(&pool, execution_id)
        .await
        .expect("Migration failed");

    assert_eq!(result.migrated, 4);

    let entries = get_log_entries(&pool, execution_id).await;
    assert_eq!(entries.len(), 4);
}

#[tokio::test]
async fn test_migrate_preserves_order() {
    let (pool, _temp_dir) = setup_test_db().await;
    let execution_id = create_test_execution(&pool).await;

    // Insert multiple records to test ordering
    for i in 0..10 {
        insert_jsonl_logs(
            &pool,
            execution_id,
            &format!(r#"{{"Stdout":"Message {}"}}"#, i),
        )
        .await;
    }

    let result = services::services::log_migration::migrate_execution_logs(&pool, execution_id)
        .await
        .expect("Migration failed");

    assert_eq!(result.migrated, 10);

    let entries = get_log_entries(&pool, execution_id).await;
    assert_eq!(entries.len(), 10);

    // Verify order is preserved
    for i in 0..10 {
        assert_eq!(entries[i].content, format!("Message {}", i));
    }
}

#[tokio::test]
async fn test_migrate_no_logs() {
    let (pool, _temp_dir) = setup_test_db().await;
    let execution_id = create_test_execution(&pool).await;

    // No logs inserted

    let result = services::services::log_migration::migrate_execution_logs(&pool, execution_id)
        .await
        .expect("Migration failed");

    assert_eq!(result.migrated, 0);
    assert_eq!(result.skipped, 0);
    assert_eq!(result.errors, 0);
}

#[tokio::test]
async fn test_migrate_all_executions() {
    let (pool, _temp_dir) = setup_test_db().await;

    // Create multiple executions with logs
    let exec1 = create_test_execution(&pool).await;
    let exec2 = create_test_execution(&pool).await;
    let exec3 = create_test_execution(&pool).await;

    insert_jsonl_logs(&pool, exec1, r#"{"Stdout":"Exec 1 Log 1"}"#).await;
    insert_jsonl_logs(&pool, exec1, r#"{"Stdout":"Exec 1 Log 2"}"#).await;
    insert_jsonl_logs(&pool, exec2, r#"{"Stderr":"Exec 2 Error"}"#).await;
    // exec3 has no logs - it won't be processed since there's nothing to migrate

    let result = services::services::log_migration::migrate_all_logs(&pool)
        .await
        .expect("Migration failed");

    // Only 2 executions have logs to migrate
    assert_eq!(result.executions_processed, 2);
    assert_eq!(result.total_migrated, 3);

    // Verify isolation
    assert_eq!(count_log_entries(&pool, exec1).await, 2);
    assert_eq!(count_log_entries(&pool, exec2).await, 1);
    assert_eq!(count_log_entries(&pool, exec3).await, 0);
}

#[tokio::test]
async fn test_migrate_idempotent() {
    let (pool, _temp_dir) = setup_test_db().await;
    let execution_id = create_test_execution(&pool).await;

    insert_jsonl_logs(&pool, execution_id, r#"{"Stdout":"Test message"}"#).await;

    // First migration
    let result1 = services::services::log_migration::migrate_execution_logs(&pool, execution_id)
        .await
        .expect("First migration failed");
    assert_eq!(result1.migrated, 1);

    // Second migration should skip already migrated logs
    let result2 = services::services::log_migration::migrate_execution_logs(&pool, execution_id)
        .await
        .expect("Second migration failed");
    assert_eq!(result2.skipped, 1);
    assert_eq!(result2.migrated, 0);

    // Should still only have 1 entry
    assert_eq!(count_log_entries(&pool, execution_id).await, 1);
}

#[tokio::test]
async fn test_dry_run_mode() {
    let (pool, _temp_dir) = setup_test_db().await;
    let execution_id = create_test_execution(&pool).await;

    insert_jsonl_logs(&pool, execution_id, r#"{"Stdout":"Test message"}"#).await;

    // Dry run should not insert entries
    let result = services::services::log_migration::migrate_execution_logs_dry_run(&pool, execution_id)
        .await
        .expect("Dry run failed");

    assert_eq!(result.would_migrate, 1);
    assert_eq!(result.would_skip, 0);
    assert_eq!(result.errors, 0);

    // No entries should be in the database
    assert_eq!(count_log_entries(&pool, execution_id).await, 0);
}
