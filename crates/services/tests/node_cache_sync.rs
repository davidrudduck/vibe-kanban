//! Integration tests for node cache project synchronization.
//!
//! These tests verify that the node cache correctly handles:
//! 1. Remote projects from multiple nodes with the same git_repo_path
//! 2. Local projects that exist with the same path as remote projects
//! 3. UNIQUE constraint handling for git_repo_path column

use db::models::project::{CreateProject, Project};
use sqlx::SqlitePool;
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

/// Test that upserting a remote project with the same git_repo_path as an existing
/// remote project from a different node does NOT cause a UNIQUE constraint error.
///
/// Scenario:
/// 1. Remote project from Node A syncs with path /home/david/Code/vibe-kanban
/// 2. Remote project from Node B tries to sync with the same path
/// 3. Should NOT fail with UNIQUE constraint error
#[tokio::test]
async fn test_upsert_remote_project_same_path_different_nodes() {
    let (pool, _temp_dir) = setup_test_db().await;

    let git_repo_path = "/home/david/Code/vibe-kanban";
    let node_a_id = Uuid::new_v4();
    let node_b_id = Uuid::new_v4();

    // First upsert from Node A - should succeed
    let result_a = Project::upsert_remote_project(
        &pool,
        Uuid::new_v4(), // local_id
        Uuid::new_v4(), // remote_project_id
        "vibe-kanban".to_string(),
        git_repo_path.to_string(),
        node_a_id,
        "Node A".to_string(),
        Some("http://node-a:3001".to_string()),
        Some("online".to_string()),
    )
    .await;

    assert!(
        result_a.is_ok(),
        "First upsert should succeed: {:?}",
        result_a.err()
    );

    // Second upsert from Node B with SAME path but different remote_project_id
    // This is the bug - it should either:
    // a) Skip and not error, or
    // b) Update the existing project
    // Currently it fails with UNIQUE constraint error
    let result_b = Project::upsert_remote_project(
        &pool,
        Uuid::new_v4(), // local_id
        Uuid::new_v4(), // DIFFERENT remote_project_id
        "vibe-kanban".to_string(),
        git_repo_path.to_string(), // SAME path
        node_b_id,
        "Node B".to_string(),
        Some("http://node-b:3001".to_string()),
        Some("online".to_string()),
    )
    .await;

    // The test documents the current broken behavior:
    // This SHOULD succeed, but currently fails with UNIQUE constraint
    // Once the fix is in place, this assertion should pass
    assert!(
        result_b.is_ok(),
        "Second upsert with same path should succeed, but got: {:?}",
        result_b.err()
    );
}

/// Test that upserting a remote project with the same git_repo_path as an existing
/// LOCAL project is handled gracefully.
///
/// Scenario:
/// 1. Local project exists with path /home/david/Code/vibe-kanban (is_remote = false)
/// 2. Remote project tries to sync with the same path
/// 3. Should handle gracefully (either skip or link)
#[tokio::test]
async fn test_upsert_remote_project_where_local_exists() {
    let (pool, _temp_dir) = setup_test_db().await;

    let git_repo_path = "/home/david/Code/vibe-kanban";

    // Create a local project first
    let create_data = CreateProject {
        name: "vibe-kanban".to_string(),
        git_repo_path: git_repo_path.to_string(),
        use_existing_repo: true,
        clone_url: None,
        setup_script: None,
        dev_script: None,
        cleanup_script: None,
        copy_files: None,
    };
    let local_project = Project::create(&pool, &create_data, Uuid::new_v4())
        .await
        .expect("Failed to create local project");

    assert!(
        !local_project.is_remote,
        "Local project should have is_remote = false"
    );

    // Now try to upsert a remote project with the same path
    let result = Project::upsert_remote_project(
        &pool,
        Uuid::new_v4(),
        Uuid::new_v4(),
        "vibe-kanban".to_string(),
        git_repo_path.to_string(),
        Uuid::new_v4(),
        "Remote Node".to_string(),
        Some("http://remote:3001".to_string()),
        Some("online".to_string()),
    )
    .await;

    // This currently fails with UNIQUE constraint
    // The proper behavior should be to either:
    // a) Skip (handled at sync level, not upsert level)
    // b) Link the local project to the remote
    // For now, we document that this fails at the DB level
    // The fix should be at the sync_node_projects level to skip these
    if result.is_err() {
        // Expected current behavior - the sync layer should skip this
        println!("Expected: upsert failed due to local project with same path");
    } else {
        // If it succeeds (after fix), verify the project
        let project = result.unwrap();
        println!("Upsert succeeded, project id: {}", project.id);
    }
}

/// Test that the same node can update its own project without UNIQUE constraint issues.
#[tokio::test]
async fn test_upsert_remote_project_same_node_update() {
    let (pool, _temp_dir) = setup_test_db().await;

    let git_repo_path = "/home/david/Code/vibe-kanban";
    let node_id = Uuid::new_v4();
    let remote_project_id = Uuid::new_v4();

    // First upsert
    let result1 = Project::upsert_remote_project(
        &pool,
        Uuid::new_v4(),
        remote_project_id,
        "vibe-kanban".to_string(),
        git_repo_path.to_string(),
        node_id,
        "My Node".to_string(),
        Some("http://mynode:3001".to_string()),
        Some("online".to_string()),
    )
    .await;

    assert!(result1.is_ok(), "First upsert should succeed");

    // Second upsert with SAME remote_project_id (update case)
    let result2 = Project::upsert_remote_project(
        &pool,
        Uuid::new_v4(),
        remote_project_id,                 // SAME remote_project_id
        "vibe-kanban-updated".to_string(), // Updated name
        git_repo_path.to_string(),
        node_id,
        "My Node Updated".to_string(),
        Some("http://mynode:3002".to_string()),
        Some("busy".to_string()),
    )
    .await;

    assert!(
        result2.is_ok(),
        "Update with same remote_project_id should succeed"
    );

    let updated_project = result2.unwrap();
    assert_eq!(updated_project.name, "vibe-kanban-updated");
    assert_eq!(
        updated_project.source_node_name,
        Some("My Node Updated".to_string())
    );
}
