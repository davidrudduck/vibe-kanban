//! Tests for PR auto-discovery functionality.
//!
//! These tests verify that the PR monitor can discover PRs created by agents
//! that haven't been tracked yet.

use std::str::FromStr;

use db::models::{
    merge::Merge,
    project::{CreateProject, Project},
    task::{CreateTask, Task, TaskStatus},
    task_attempt::{CreateTaskAttempt, TaskAttempt},
};
use executors::executors::BaseCodingAgent;
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
};
use tempfile::TempDir;
use uuid::Uuid;

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

    // Run migrations from the db crate
    sqlx::migrate!("../db/migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    (pool, temp_dir)
}

/// Helper to create a test project with GitHub enabled
async fn create_test_project_with_github(
    pool: &SqlitePool,
    name: &str,
    git_repo_path: &str,
) -> Project {
    let id = Uuid::new_v4();
    let project = Project::create(
        pool,
        &CreateProject {
            name: name.to_string(),
            git_repo_path: git_repo_path.to_string(),
            use_existing_repo: true,
            clone_url: None,
            setup_script: None,
            dev_script: None,
            cleanup_script: None,
            copy_files: None,
        },
        id,
    )
    .await
    .expect("Failed to create project");

    // Enable GitHub for this project
    Project::set_github_enabled(
        pool,
        project.id,
        true,
        Some("test-owner".to_string()),
        Some("test-repo".to_string()),
    )
    .await
    .expect("Failed to enable GitHub");

    // Refetch to get updated fields
    Project::find_by_id(pool, project.id)
        .await
        .expect("Failed to refetch project")
        .expect("Project not found")
}

/// Helper to create a test task
async fn create_test_task(pool: &SqlitePool, project_id: Uuid, title: &str) -> Task {
    let id = Uuid::new_v4();
    Task::create(
        pool,
        &CreateTask::from_title_description(project_id, title.to_string(), None),
        id,
    )
    .await
    .expect("Failed to create task")
}

/// Helper to create a test task attempt
async fn create_test_attempt(
    pool: &SqlitePool,
    task_id: Uuid,
    branch: &str,
    base_branch: &str,
) -> TaskAttempt {
    let id = Uuid::new_v4();
    TaskAttempt::create(
        pool,
        &CreateTaskAttempt {
            executor: BaseCodingAgent::ClaudeCode,
            branch: branch.to_string(),
            base_branch: base_branch.to_string(),
        },
        id,
        task_id,
    )
    .await
    .expect("Failed to create task attempt")
}

#[tokio::test]
async fn test_find_active_attempts_without_pr_returns_untracked() {
    let (pool, _dir) = setup_test_pool().await;

    // Create a project with GitHub enabled, task, and attempt
    let project = create_test_project_with_github(&pool, "Test Project", "/tmp/test-repo").await;
    let task = create_test_task(&pool, project.id, "Test Task").await;
    let attempt = create_test_attempt(&pool, task.id, "feature/test", "main").await;

    // Query for active attempts without PR merges
    let attempts_without_pr = TaskAttempt::find_active_without_pr(&pool).await.unwrap();

    // Should find our attempt since it has no PR merge
    assert_eq!(attempts_without_pr.len(), 1);
    assert_eq!(attempts_without_pr[0].0, attempt.id);
    assert_eq!(attempts_without_pr[0].1, "feature/test");
    assert_eq!(attempts_without_pr[0].2, "test-owner");
    assert_eq!(attempts_without_pr[0].3, "test-repo");
}

#[tokio::test]
async fn test_find_active_attempts_with_pr_excluded() {
    let (pool, _dir) = setup_test_pool().await;

    // Create a project with GitHub enabled, task, and attempt
    let project = create_test_project_with_github(&pool, "Test Project", "/tmp/test-repo").await;
    let task = create_test_task(&pool, project.id, "Test Task").await;
    let attempt = create_test_attempt(&pool, task.id, "feature/test", "main").await;

    // Create a PR merge for this attempt
    Merge::create_pr(
        &pool,
        attempt.id,
        "main",
        123,
        "https://github.com/test/repo/pull/123",
    )
    .await
    .unwrap();

    // Query for active attempts without PR merges
    let attempts_without_pr = TaskAttempt::find_active_without_pr(&pool).await.unwrap();

    // Should NOT find our attempt since it has a PR merge
    assert!(attempts_without_pr.is_empty());
}

#[tokio::test]
async fn test_find_active_attempts_excludes_done_tasks() {
    let (pool, _dir) = setup_test_pool().await;

    // Create a project with GitHub enabled, task, and attempt
    let project = create_test_project_with_github(&pool, "Test Project", "/tmp/test-repo").await;
    let task = create_test_task(&pool, project.id, "Test Task").await;
    let _attempt = create_test_attempt(&pool, task.id, "feature/test", "main").await;

    // Mark the task as done
    Task::update_status(&pool, task.id, TaskStatus::Done)
        .await
        .unwrap();

    // Query for active attempts without PR merges
    let attempts_without_pr = TaskAttempt::find_active_without_pr(&pool).await.unwrap();

    // Should NOT find our attempt since task is done
    assert!(attempts_without_pr.is_empty());
}

#[tokio::test]
async fn test_find_active_attempts_includes_in_progress_and_review() {
    let (pool, _dir) = setup_test_pool().await;

    // Create a project with GitHub enabled
    let project = create_test_project_with_github(&pool, "Test Project", "/tmp/test-repo").await;

    // Create tasks with different statuses
    let task_todo = create_test_task(&pool, project.id, "Todo Task").await;
    let task_in_progress = create_test_task(&pool, project.id, "In Progress Task").await;
    let task_in_review = create_test_task(&pool, project.id, "In Review Task").await;
    let task_done = create_test_task(&pool, project.id, "Done Task").await;

    // Create attempts for each
    let _attempt_todo = create_test_attempt(&pool, task_todo.id, "feat/todo", "main").await;
    let _attempt_in_progress =
        create_test_attempt(&pool, task_in_progress.id, "feat/progress", "main").await;
    let _attempt_in_review =
        create_test_attempt(&pool, task_in_review.id, "feat/review", "main").await;
    let _attempt_done = create_test_attempt(&pool, task_done.id, "feat/done", "main").await;

    // Update task statuses
    Task::update_status(&pool, task_in_progress.id, TaskStatus::InProgress)
        .await
        .unwrap();
    Task::update_status(&pool, task_in_review.id, TaskStatus::InReview)
        .await
        .unwrap();
    Task::update_status(&pool, task_done.id, TaskStatus::Done)
        .await
        .unwrap();

    // Query for active attempts
    let attempts = TaskAttempt::find_active_without_pr(&pool).await.unwrap();

    // Should find 3 attempts (todo, in_progress, in_review) but NOT done
    assert_eq!(attempts.len(), 3);

    // Verify we got the expected branches
    let branches: Vec<_> = attempts
        .iter()
        .map(|(_, branch, _, _)| branch.as_str())
        .collect();
    assert!(branches.contains(&"feat/todo"));
    assert!(branches.contains(&"feat/progress"));
    assert!(branches.contains(&"feat/review"));
    assert!(!branches.contains(&"feat/done"));
}
