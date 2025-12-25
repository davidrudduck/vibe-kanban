//! Integration tests for bulk database operations.
//!
//! These tests verify the correctness of O(1) bulk operations:
//! - `Task::archive_many()` - bulk archive tasks
//! - `Task::delete_stale_remote_tasks()` - bulk delete stale remote tasks
//! - `Project::delete_stale_remote_projects()` - bulk delete stale remote projects

use std::str::FromStr;

use db::models::{
    project::{CreateProject, Project},
    task::{CreateTask, Task, TaskStatus},
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
        git_repo_path: "/tmp/test-repo".to_string(),
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

/// Create a remote task (with shared_task_id set) for delete_stale tests.
async fn create_remote_task(
    pool: &SqlitePool,
    project_id: Uuid,
    title: &str,
    shared_task_id: Uuid,
) -> Task {
    let task_id = Uuid::new_v4();
    let data = CreateTask::from_shared_task(
        project_id,
        title.to_string(),
        None,
        TaskStatus::Todo,
        shared_task_id,
    );

    // Create the task first
    let _task = Task::create(pool, &data, task_id)
        .await
        .expect("Failed to create remote task");

    // Mark it as remote (is_remote = 1) using raw query
    sqlx::query("UPDATE tasks SET is_remote = 1 WHERE id = ?")
        .bind(task_id)
        .execute(pool)
        .await
        .expect("Failed to mark task as remote");

    // Fetch and return the updated task
    Task::find_by_id(pool, task_id)
        .await
        .expect("Failed to fetch task")
        .expect("Task not found")
}

// =============================================================================
// Task::archive_many() tests
// =============================================================================

#[tokio::test]
async fn test_archive_many_bulk_update() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;

    // Create 5 tasks
    let mut task_ids = Vec::new();
    for i in 1..=5 {
        let task = create_test_task(&pool, project.id, &format!("Task {}", i)).await;
        task_ids.push(task.id);
    }

    // Verify none are archived
    for &id in &task_ids {
        let task = Task::find_by_id(&pool, id).await.unwrap().unwrap();
        assert!(
            task.archived_at.is_none(),
            "Task should not be archived initially"
        );
    }

    // Archive all 5 tasks
    let rows_affected = Task::archive_many(&pool, &task_ids)
        .await
        .expect("archive_many failed");

    // Verify count
    assert_eq!(rows_affected, 5, "Should have archived 5 tasks");

    // Verify all are now archived
    for &id in &task_ids {
        let task = Task::find_by_id(&pool, id).await.unwrap().unwrap();
        assert!(task.archived_at.is_some(), "Task should be archived");
    }
}

#[tokio::test]
async fn test_archive_many_empty_list_noop() {
    let (pool, _temp_dir) = setup_test_pool().await;

    // Archive with empty list
    let rows_affected = Task::archive_many(&pool, &[])
        .await
        .expect("archive_many with empty list should not fail");

    assert_eq!(rows_affected, 0, "Should return 0 for empty list");
}

#[tokio::test]
async fn test_archive_many_partial_ids() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;

    // Create 3 tasks
    let task1 = create_test_task(&pool, project.id, "Task 1").await;
    let task2 = create_test_task(&pool, project.id, "Task 2").await;
    let task3 = create_test_task(&pool, project.id, "Task 3").await;

    // Archive with 2 valid IDs + 1 non-existent UUID
    let non_existent_id = Uuid::new_v4();
    let ids_to_archive = vec![task1.id, task2.id, non_existent_id];

    let rows_affected = Task::archive_many(&pool, &ids_to_archive)
        .await
        .expect("archive_many with partial IDs should not fail");

    // Should have archived only the 2 valid tasks
    assert_eq!(rows_affected, 2, "Should have archived 2 tasks");

    // Verify task1 and task2 are archived
    let t1 = Task::find_by_id(&pool, task1.id).await.unwrap().unwrap();
    let t2 = Task::find_by_id(&pool, task2.id).await.unwrap().unwrap();
    let t3 = Task::find_by_id(&pool, task3.id).await.unwrap().unwrap();

    assert!(t1.archived_at.is_some(), "Task 1 should be archived");
    assert!(t2.archived_at.is_some(), "Task 2 should be archived");
    assert!(t3.archived_at.is_none(), "Task 3 should NOT be archived");
}

#[tokio::test]
async fn test_archive_many_already_archived_tasks() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;

    // Create 2 tasks
    let task1 = create_test_task(&pool, project.id, "Task 1").await;
    let task2 = create_test_task(&pool, project.id, "Task 2").await;

    // Archive task1 first
    Task::archive_many(&pool, &[task1.id])
        .await
        .expect("First archive should succeed");

    // Archive both (task1 already archived)
    let rows_affected = Task::archive_many(&pool, &[task1.id, task2.id])
        .await
        .expect("Re-archive should not fail");

    // Both rows are "affected" by the UPDATE (even if value doesn't change)
    // SQLite returns rows matched, not rows changed
    assert_eq!(rows_affected, 2, "Both rows should be affected");

    // Both should be archived
    let t1 = Task::find_by_id(&pool, task1.id).await.unwrap().unwrap();
    let t2 = Task::find_by_id(&pool, task2.id).await.unwrap().unwrap();

    assert!(t1.archived_at.is_some(), "Task 1 should be archived");
    assert!(t2.archived_at.is_some(), "Task 2 should be archived");
}

// =============================================================================
// Task::delete_stale_remote_tasks() tests
// =============================================================================

#[tokio::test]
async fn test_delete_stale_remote_tasks_bulk_delete() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;

    // Create 5 remote tasks with shared_task_ids
    let shared_id_1 = Uuid::new_v4();
    let shared_id_2 = Uuid::new_v4();
    let shared_id_3 = Uuid::new_v4();
    let shared_id_4 = Uuid::new_v4();
    let shared_id_5 = Uuid::new_v4();

    let _task1 = create_remote_task(&pool, project.id, "Remote Task 1", shared_id_1).await;
    let task2 = create_remote_task(&pool, project.id, "Remote Task 2", shared_id_2).await;
    let task3 = create_remote_task(&pool, project.id, "Remote Task 3", shared_id_3).await;
    let _task4 = create_remote_task(&pool, project.id, "Remote Task 4", shared_id_4).await;
    let task5 = create_remote_task(&pool, project.id, "Remote Task 5", shared_id_5).await;

    // Active shared_task_ids (these should NOT be deleted)
    let active_ids = vec![shared_id_2, shared_id_3, shared_id_5];

    // Delete stale tasks (1 and 4 should be deleted)
    let rows_deleted = Task::delete_stale_remote_tasks(&pool, project.id, &active_ids)
        .await
        .expect("delete_stale_remote_tasks failed");

    assert_eq!(rows_deleted, 2, "Should have deleted 2 stale tasks");

    // Verify remaining tasks
    let remaining = vec![task2.id, task3.id, task5.id];
    for id in remaining {
        let task = Task::find_by_id(&pool, id).await.unwrap();
        assert!(task.is_some(), "Active task should still exist");
    }
}

#[tokio::test]
async fn test_delete_stale_remote_tasks_empty_active_list_noop() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;

    // Create 2 remote tasks
    let shared_id_1 = Uuid::new_v4();
    let shared_id_2 = Uuid::new_v4();

    let task1 = create_remote_task(&pool, project.id, "Remote Task 1", shared_id_1).await;
    let task2 = create_remote_task(&pool, project.id, "Remote Task 2", shared_id_2).await;

    // Call with empty active list - should NOT delete anything (safety check)
    let rows_deleted = Task::delete_stale_remote_tasks(&pool, project.id, &[])
        .await
        .expect("delete_stale_remote_tasks with empty list should not fail");

    assert_eq!(
        rows_deleted, 0,
        "Should return 0 for empty active list (safety)"
    );

    // Both tasks should still exist
    assert!(Task::find_by_id(&pool, task1.id).await.unwrap().is_some());
    assert!(Task::find_by_id(&pool, task2.id).await.unwrap().is_some());
}

#[tokio::test]
async fn test_delete_stale_remote_tasks_only_affects_remote_tasks() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;

    // Create a local task (no shared_task_id, is_remote = false)
    let local_task = create_test_task(&pool, project.id, "Local Task").await;

    // Create a remote task
    let shared_id = Uuid::new_v4();
    let remote_task = create_remote_task(&pool, project.id, "Remote Task", shared_id).await;

    // Delete stale with active list that doesn't include the remote task
    let unrelated_id = Uuid::new_v4();
    let rows_deleted = Task::delete_stale_remote_tasks(&pool, project.id, &[unrelated_id])
        .await
        .expect("delete_stale_remote_tasks failed");

    // Only the remote task should be deleted
    assert_eq!(rows_deleted, 1, "Should have deleted 1 remote task");

    // Local task should still exist
    assert!(
        Task::find_by_id(&pool, local_task.id)
            .await
            .unwrap()
            .is_some(),
        "Local task should not be affected"
    );

    // Remote task should be deleted
    assert!(
        Task::find_by_id(&pool, remote_task.id)
            .await
            .unwrap()
            .is_none(),
        "Remote task should be deleted"
    );
}

#[tokio::test]
async fn test_delete_stale_remote_tasks_project_isolation() {
    let (pool, _temp_dir) = setup_test_pool().await;

    // Create two projects
    let project1 = create_test_project(&pool).await;

    let project2_id = Uuid::new_v4();
    let project2_data = CreateProject {
        name: "Test Project 2".to_string(),
        git_repo_path: "/tmp/test-repo-2".to_string(),
        use_existing_repo: true,
        clone_url: None,
        setup_script: None,
        dev_script: None,
        cleanup_script: None,
        copy_files: None,
    };
    let _project2 = Project::create(&pool, &project2_data, project2_id)
        .await
        .expect("Failed to create project 2");

    // Create remote tasks in both projects
    let shared_id_p1 = Uuid::new_v4();
    let shared_id_p2 = Uuid::new_v4();

    let _task_p1 = create_remote_task(&pool, project1.id, "P1 Remote Task", shared_id_p1).await;
    let task_p2 = create_remote_task(&pool, project2_id, "P2 Remote Task", shared_id_p2).await;

    // Delete stale tasks only in project1
    let unrelated_id = Uuid::new_v4();
    let rows_deleted = Task::delete_stale_remote_tasks(&pool, project1.id, &[unrelated_id])
        .await
        .expect("delete_stale_remote_tasks failed");

    assert_eq!(rows_deleted, 1, "Should have deleted 1 task from project1");

    // Project2's task should still exist
    assert!(
        Task::find_by_id(&pool, task_p2.id).await.unwrap().is_some(),
        "Project 2 task should not be affected"
    );
}

// =============================================================================
// Project::delete_stale_remote_projects() tests
// =============================================================================

/// Create a remote project for testing.
async fn create_remote_project(pool: &SqlitePool, name: &str, remote_project_id: Uuid) -> Project {
    let local_id = Uuid::new_v4();
    let source_node_id = Uuid::new_v4();

    Project::upsert_remote_project(
        pool,
        local_id,
        remote_project_id,
        name.to_string(),
        format!("/tmp/remote-repo-{}", name),
        source_node_id,
        "test-node".to_string(),
        None,
        Some("online".to_string()),
    )
    .await
    .expect("Failed to create remote project")
}

#[tokio::test]
async fn test_delete_stale_remote_projects_bulk_delete() {
    let (pool, _temp_dir) = setup_test_pool().await;

    // Create 4 remote projects
    let remote_id_1 = Uuid::new_v4();
    let remote_id_2 = Uuid::new_v4();
    let remote_id_3 = Uuid::new_v4();
    let remote_id_4 = Uuid::new_v4();

    let _project1 = create_remote_project(&pool, "Remote Project 1", remote_id_1).await;
    let project2 = create_remote_project(&pool, "Remote Project 2", remote_id_2).await;
    let _project3 = create_remote_project(&pool, "Remote Project 3", remote_id_3).await;
    let project4 = create_remote_project(&pool, "Remote Project 4", remote_id_4).await;

    // Active remote_project_ids (these should NOT be deleted)
    let active_ids = vec![remote_id_2, remote_id_4];

    // Delete stale projects (1 and 3 should be deleted)
    let rows_deleted = Project::delete_stale_remote_projects(&pool, &active_ids)
        .await
        .expect("delete_stale_remote_projects failed");

    assert_eq!(rows_deleted, 2, "Should have deleted 2 stale projects");

    // Verify remaining projects
    assert!(
        Project::find_by_id(&pool, project2.id)
            .await
            .unwrap()
            .is_some(),
        "Project 2 should still exist"
    );
    assert!(
        Project::find_by_id(&pool, project4.id)
            .await
            .unwrap()
            .is_some(),
        "Project 4 should still exist"
    );
}

#[tokio::test]
async fn test_delete_stale_remote_projects_empty_active_list_noop() {
    let (pool, _temp_dir) = setup_test_pool().await;

    // Create 2 remote projects
    let remote_id_1 = Uuid::new_v4();
    let remote_id_2 = Uuid::new_v4();

    let project1 = create_remote_project(&pool, "Remote Project 1", remote_id_1).await;
    let project2 = create_remote_project(&pool, "Remote Project 2", remote_id_2).await;

    // Call with empty active list - should NOT delete anything (safety check)
    let rows_deleted = Project::delete_stale_remote_projects(&pool, &[])
        .await
        .expect("delete_stale_remote_projects with empty list should not fail");

    assert_eq!(
        rows_deleted, 0,
        "Should return 0 for empty active list (safety)"
    );

    // Both projects should still exist
    assert!(
        Project::find_by_id(&pool, project1.id)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        Project::find_by_id(&pool, project2.id)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn test_delete_stale_remote_projects_only_affects_remote() {
    let (pool, _temp_dir) = setup_test_pool().await;

    // Create a local project
    let local_project = create_test_project(&pool).await;

    // Create a remote project
    let remote_id = Uuid::new_v4();
    let remote_project = create_remote_project(&pool, "Remote Project", remote_id).await;

    // Delete stale with active list that doesn't include the remote project
    let unrelated_id = Uuid::new_v4();
    let rows_deleted = Project::delete_stale_remote_projects(&pool, &[unrelated_id])
        .await
        .expect("delete_stale_remote_projects failed");

    // Only the remote project should be deleted
    assert_eq!(rows_deleted, 1, "Should have deleted 1 remote project");

    // Local project should still exist
    assert!(
        Project::find_by_id(&pool, local_project.id)
            .await
            .unwrap()
            .is_some(),
        "Local project should not be affected"
    );

    // Remote project should be deleted
    assert!(
        Project::find_by_id(&pool, remote_project.id)
            .await
            .unwrap()
            .is_none(),
        "Remote project should be deleted"
    );
}
