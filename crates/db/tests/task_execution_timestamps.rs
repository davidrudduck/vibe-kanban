//! Integration tests for task execution timestamp fields.
//!
//! These tests verify the correctness of `latest_execution_started_at` and
//! `latest_execution_completed_at` fields added to `TaskWithAttemptStatus` and
//! `TaskWithProjectInfo` structs.
//!
//! The timestamps are computed using MAX() subqueries on execution_processes
//! filtered by `run_reason = 'codingagent'` and `dropped = FALSE`.

use std::str::FromStr;

use chrono::{DateTime, Duration, Utc};
use db::models::{
    all_tasks::AllTasksResponse,
    project::{CreateProject, Project},
    task::{CreateTask, Task},
};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
};
use tempfile::TempDir;
use uuid::Uuid;

/// Create an in-memory SQLite pool with migrations applied.
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

/// Create a test project for task tests.
async fn create_test_project(pool: &SqlitePool) -> Project {
    let project_id = Uuid::new_v4();
    let data = CreateProject {
        name: "Test Project".to_string(),
        git_repo_path: format!("/tmp/test-repo-{}", project_id),
        use_existing_repo: true,
        clone_url: None,
        setup_script: None,
        dev_script: None,
        cleanup_script: None,
        copy_files: None,
    };
    Project::create(pool, &data, project_id)
        .await
        .expect("Failed to create test project")
}

/// Create a test task in the given project.
async fn create_test_task(pool: &SqlitePool, project_id: Uuid, title: &str) -> Task {
    let task_id = Uuid::new_v4();
    let data = CreateTask::from_title_description(project_id, title.to_string(), None);
    Task::create(pool, &data, task_id)
        .await
        .expect("Failed to create test task")
}

/// Create a task attempt for testing.
async fn create_task_attempt(pool: &SqlitePool, task_id: Uuid) -> Uuid {
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
    attempt_id
}

/// Create an execution process for the given attempt with specific timestamps.
/// Returns the execution process ID.
async fn create_execution_process(
    pool: &SqlitePool,
    task_attempt_id: Uuid,
    run_reason: &str,
    started_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    dropped: bool,
) -> Uuid {
    let execution_id = Uuid::new_v4();
    let status = if completed_at.is_some() {
        "completed"
    } else {
        "running"
    };

    sqlx::query(
        r#"INSERT INTO execution_processes
           (id, task_attempt_id, status, run_reason, executor_action, started_at, completed_at, dropped)
           VALUES ($1, $2, $3, $4, '{}', $5, $6, $7)"#,
    )
    .bind(execution_id)
    .bind(task_attempt_id)
    .bind(status)
    .bind(run_reason)
    .bind(started_at)
    .bind(completed_at)
    .bind(dropped)
    .execute(pool)
    .await
    .expect("Failed to create execution process");
    execution_id
}

// =============================================================================
// TaskWithAttemptStatus execution timestamp tests
// =============================================================================

#[tokio::test]
async fn test_task_without_attempts_has_null_timestamps() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;

    // Create a task with no attempts
    let _task = create_test_task(&pool, project.id, "Task without attempts").await;

    // Fetch tasks with attempt status
    let tasks = Task::find_by_project_id_with_attempt_status(&pool, project.id, false)
        .await
        .expect("Failed to fetch tasks");

    assert_eq!(tasks.len(), 1);
    let task_status = &tasks[0];

    // Both timestamps should be None
    assert!(
        task_status.latest_execution_started_at.is_none(),
        "latest_execution_started_at should be None for task without attempts"
    );
    assert!(
        task_status.latest_execution_completed_at.is_none(),
        "latest_execution_completed_at should be None for task without attempts"
    );
}

#[tokio::test]
async fn test_task_with_running_execution_has_started_at_only() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;

    // Create a task with a running execution
    let task = create_test_task(&pool, project.id, "Task with running execution").await;
    let attempt_id = create_task_attempt(&pool, task.id).await;

    let started_at = Utc::now();
    create_execution_process(
        &pool,
        attempt_id,
        "codingagent",
        started_at,
        None, // No completed_at (running)
        false,
    )
    .await;

    // Fetch tasks with attempt status
    let tasks = Task::find_by_project_id_with_attempt_status(&pool, project.id, false)
        .await
        .expect("Failed to fetch tasks");

    assert_eq!(tasks.len(), 1);
    let task_status = &tasks[0];

    // started_at should be set, completed_at should be None
    assert!(
        task_status.latest_execution_started_at.is_some(),
        "latest_execution_started_at should be Some for running execution"
    );
    assert!(
        task_status.latest_execution_completed_at.is_none(),
        "latest_execution_completed_at should be None for running execution"
    );

    // Verify the timestamp is approximately correct (within 5 seconds)
    let diff = (task_status.latest_execution_started_at.unwrap() - started_at).num_seconds();
    assert!(
        diff.abs() < 5,
        "Timestamp should be approximately the started_at time"
    );
}

#[tokio::test]
async fn test_task_with_completed_execution_has_both_timestamps() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;

    // Create a task with a completed execution
    let task = create_test_task(&pool, project.id, "Task with completed execution").await;
    let attempt_id = create_task_attempt(&pool, task.id).await;

    let started_at = Utc::now() - Duration::hours(1);
    let completed_at = Utc::now();
    create_execution_process(
        &pool,
        attempt_id,
        "codingagent",
        started_at,
        Some(completed_at),
        false,
    )
    .await;

    // Fetch tasks with attempt status
    let tasks = Task::find_by_project_id_with_attempt_status(&pool, project.id, false)
        .await
        .expect("Failed to fetch tasks");

    assert_eq!(tasks.len(), 1);
    let task_status = &tasks[0];

    // Both timestamps should be set
    assert!(
        task_status.latest_execution_started_at.is_some(),
        "latest_execution_started_at should be Some for completed execution"
    );
    assert!(
        task_status.latest_execution_completed_at.is_some(),
        "latest_execution_completed_at should be Some for completed execution"
    );

    // Verify the timestamps are approximately correct
    let started_diff =
        (task_status.latest_execution_started_at.unwrap() - started_at).num_seconds();
    let completed_diff =
        (task_status.latest_execution_completed_at.unwrap() - completed_at).num_seconds();

    assert!(
        started_diff.abs() < 5,
        "Started timestamp should be approximately correct"
    );
    assert!(
        completed_diff.abs() < 5,
        "Completed timestamp should be approximately correct"
    );
}

#[tokio::test]
async fn test_uses_latest_execution_ignoring_dropped() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;

    // Create a task with multiple executions, some dropped
    let task = create_test_task(&pool, project.id, "Task with multiple executions").await;
    let attempt_id = create_task_attempt(&pool, task.id).await;

    // First execution: older, not dropped (should be ignored because there's a newer one)
    let old_started = Utc::now() - Duration::hours(3);
    let old_completed = Utc::now() - Duration::hours(2);
    create_execution_process(
        &pool,
        attempt_id,
        "codingagent",
        old_started,
        Some(old_completed),
        false,
    )
    .await;

    // Second execution: newest but dropped (should be ignored)
    let dropped_started = Utc::now() - Duration::minutes(30);
    let dropped_completed = Utc::now() - Duration::minutes(10);
    create_execution_process(
        &pool,
        attempt_id,
        "codingagent",
        dropped_started,
        Some(dropped_completed),
        true, // dropped
    )
    .await;

    // Third execution: middle time, not dropped (should be used because newer than first, dropped not used)
    let expected_started = Utc::now() - Duration::hours(1);
    let expected_completed = Utc::now() - Duration::minutes(45);
    create_execution_process(
        &pool,
        attempt_id,
        "codingagent",
        expected_started,
        Some(expected_completed),
        false,
    )
    .await;

    // Fetch tasks with attempt status
    let tasks = Task::find_by_project_id_with_attempt_status(&pool, project.id, false)
        .await
        .expect("Failed to fetch tasks");

    assert_eq!(tasks.len(), 1);
    let task_status = &tasks[0];

    // Should use MAX of non-dropped executions
    assert!(
        task_status.latest_execution_started_at.is_some(),
        "latest_execution_started_at should be Some"
    );
    assert!(
        task_status.latest_execution_completed_at.is_some(),
        "latest_execution_completed_at should be Some"
    );

    // The latest timestamps from non-dropped should be the third execution
    // (expected_started is newer than old_started, dropped execution is ignored)
    let actual_started = task_status.latest_execution_started_at.unwrap();
    let actual_completed = task_status.latest_execution_completed_at.unwrap();

    // The MAX() should pick expected_started (third execution) for started_at
    // and expected_completed (third execution) for completed_at
    // (since third execution is the latest non-dropped)
    assert!(
        (actual_started - expected_started).num_seconds().abs() < 5,
        "Should use the latest non-dropped execution's started_at. Expected: {:?}, Got: {:?}",
        expected_started,
        actual_started
    );
    assert!(
        (actual_completed - expected_completed).num_seconds().abs() < 5,
        "Should use the latest non-dropped execution's completed_at. Expected: {:?}, Got: {:?}",
        expected_completed,
        actual_completed
    );
}

#[tokio::test]
async fn test_only_uses_codingagent_run_reason() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;

    // Create a task with executions of different run_reasons
    let task = create_test_task(&pool, project.id, "Task with different run reasons").await;
    let attempt_id = create_task_attempt(&pool, task.id).await;

    // Setup script execution (should be ignored)
    let setup_started = Utc::now() - Duration::hours(2);
    let setup_completed = Utc::now() - Duration::hours(1);
    create_execution_process(
        &pool,
        attempt_id,
        "setupscript",
        setup_started,
        Some(setup_completed),
        false,
    )
    .await;

    // Dev server execution (should be ignored)
    let dev_started = Utc::now() - Duration::minutes(30);
    create_execution_process(&pool, attempt_id, "devserver", dev_started, None, false).await;

    // Coding agent execution (should be used)
    let coding_started = Utc::now() - Duration::minutes(10);
    let coding_completed = Utc::now() - Duration::minutes(5);
    create_execution_process(
        &pool,
        attempt_id,
        "codingagent",
        coding_started,
        Some(coding_completed),
        false,
    )
    .await;

    // Fetch tasks with attempt status
    let tasks = Task::find_by_project_id_with_attempt_status(&pool, project.id, false)
        .await
        .expect("Failed to fetch tasks");

    assert_eq!(tasks.len(), 1);
    let task_status = &tasks[0];

    // Should only use codingagent timestamps
    assert!(task_status.latest_execution_started_at.is_some());
    assert!(task_status.latest_execution_completed_at.is_some());

    let actual_started = task_status.latest_execution_started_at.unwrap();
    let actual_completed = task_status.latest_execution_completed_at.unwrap();

    // Should match coding agent execution, not setup or dev server
    assert!(
        (actual_started - coding_started).num_seconds().abs() < 5,
        "Should use codingagent started_at, not setupscript or devserver"
    );
    assert!(
        (actual_completed - coding_completed).num_seconds().abs() < 5,
        "Should use codingagent completed_at"
    );
}

// =============================================================================
// AllTasksResponse (TaskWithProjectInfo) execution timestamp tests
// =============================================================================

#[tokio::test]
async fn test_all_tasks_without_attempts_has_null_timestamps() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;

    // Create a task with no attempts
    let _task = create_test_task(&pool, project.id, "All tasks - no attempts").await;

    // Fetch all tasks
    let response = AllTasksResponse::fetch(&pool, false)
        .await
        .expect("Failed to fetch all tasks");

    assert_eq!(response.tasks.len(), 1);
    let task_info = &response.tasks[0];

    // Both timestamps should be None
    assert!(
        task_info.latest_execution_started_at.is_none(),
        "latest_execution_started_at should be None for task without attempts"
    );
    assert!(
        task_info.latest_execution_completed_at.is_none(),
        "latest_execution_completed_at should be None for task without attempts"
    );
}

#[tokio::test]
async fn test_all_tasks_with_completed_execution_has_both_timestamps() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;

    // Create a task with a completed execution
    let task = create_test_task(&pool, project.id, "All tasks - completed execution").await;
    let attempt_id = create_task_attempt(&pool, task.id).await;

    let started_at = Utc::now() - Duration::hours(1);
    let completed_at = Utc::now();
    create_execution_process(
        &pool,
        attempt_id,
        "codingagent",
        started_at,
        Some(completed_at),
        false,
    )
    .await;

    // Fetch all tasks
    let response = AllTasksResponse::fetch(&pool, false)
        .await
        .expect("Failed to fetch all tasks");

    assert_eq!(response.tasks.len(), 1);
    let task_info = &response.tasks[0];

    // Both timestamps should be set
    assert!(
        task_info.latest_execution_started_at.is_some(),
        "latest_execution_started_at should be Some for completed execution"
    );
    assert!(
        task_info.latest_execution_completed_at.is_some(),
        "latest_execution_completed_at should be Some for completed execution"
    );
}

#[tokio::test]
async fn test_all_tasks_uses_latest_execution_ignoring_dropped() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;

    // Create a task with multiple executions
    let task = create_test_task(&pool, project.id, "All tasks - multiple executions").await;
    let attempt_id = create_task_attempt(&pool, task.id).await;

    // Dropped execution (should be ignored)
    let dropped_started = Utc::now() - Duration::minutes(30);
    let dropped_completed = Utc::now() - Duration::minutes(10);
    create_execution_process(
        &pool,
        attempt_id,
        "codingagent",
        dropped_started,
        Some(dropped_completed),
        true,
    )
    .await;

    // Non-dropped execution (should be used)
    let expected_started = Utc::now() - Duration::hours(1);
    let expected_completed = Utc::now() - Duration::minutes(45);
    create_execution_process(
        &pool,
        attempt_id,
        "codingagent",
        expected_started,
        Some(expected_completed),
        false,
    )
    .await;

    // Fetch all tasks
    let response = AllTasksResponse::fetch(&pool, false)
        .await
        .expect("Failed to fetch all tasks");

    assert_eq!(response.tasks.len(), 1);
    let task_info = &response.tasks[0];

    // Should use non-dropped execution timestamps
    let actual_started = task_info.latest_execution_started_at.unwrap();
    let actual_completed = task_info.latest_execution_completed_at.unwrap();

    assert!(
        (actual_started - expected_started).num_seconds().abs() < 5,
        "Should use the non-dropped execution's started_at"
    );
    assert!(
        (actual_completed - expected_completed).num_seconds().abs() < 5,
        "Should use the non-dropped execution's completed_at"
    );
}
