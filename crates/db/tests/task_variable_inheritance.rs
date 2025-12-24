//! Integration tests for TaskVariable recursive CTE inheritance.
//!
//! These tests verify the correctness of the O(1) recursive CTE query that
//! fetches task variables with inheritance through the parent chain:
//! - `TaskVariable::find_inherited()` - finds variables with parent inheritance

use std::str::FromStr;

use db::models::{
    project::{CreateProject, Project},
    task::{CreateTask, Task},
    task_variable::{CreateTaskVariable, ResolvedVariable, TaskVariable},
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
        setup_script: None,
        dev_script: None,
        cleanup_script: None,
        copy_files: None,
    };
    Project::create(pool, &data, project_id)
        .await
        .expect("Failed to create test project")
}

/// Create a test task in the given project with optional parent.
async fn create_test_task(
    pool: &SqlitePool,
    project_id: Uuid,
    title: &str,
    parent_task_id: Option<Uuid>,
) -> Task {
    let task_id = Uuid::new_v4();
    let data = CreateTask {
        project_id,
        title: title.to_string(),
        description: None,
        status: None,
        parent_task_id,
        image_ids: None,
        shared_task_id: None,
        validation_steps: None,
    };
    Task::create(pool, &data, task_id)
        .await
        .expect("Failed to create test task")
}

/// Create a variable on a task.
async fn create_variable(
    pool: &SqlitePool,
    task_id: Uuid,
    name: &str,
    value: &str,
) -> TaskVariable {
    let data = CreateTaskVariable {
        name: name.to_string(),
        value: value.to_string(),
    };
    TaskVariable::create(pool, task_id, &data)
        .await
        .expect("Failed to create variable")
}

/// Helper to find a variable by name in a list of resolved variables.
fn find_var<'a>(vars: &'a [ResolvedVariable], name: &str) -> Option<&'a ResolvedVariable> {
    vars.iter().find(|v| v.name == name)
}

// =============================================================================
// TaskVariable::find_inherited() tests
// =============================================================================

#[tokio::test]
async fn test_find_inherited_no_parent_returns_own_vars() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;

    // Create a task with no parent
    let task = create_test_task(&pool, project.id, "Standalone Task", None).await;

    // Add 2 variables to the task
    let _var_a = create_variable(&pool, task.id, "VAR_A", "value_a").await;
    let _var_b = create_variable(&pool, task.id, "VAR_B", "value_b").await;

    // Call find_inherited
    let result = TaskVariable::find_inherited(&pool, task.id)
        .await
        .expect("find_inherited failed");

    // Should return 2 variables
    assert_eq!(result.len(), 2, "Should return 2 variables");

    // All should have inherited=false (defined directly on this task)
    let var_a = find_var(&result, "VAR_A").expect("VAR_A not found");
    let var_b = find_var(&result, "VAR_B").expect("VAR_B not found");

    assert_eq!(var_a.value, "value_a");
    assert!(!var_a.inherited, "VAR_A should not be inherited");
    assert_eq!(var_a.source_task_id, task.id);

    assert_eq!(var_b.value, "value_b");
    assert!(!var_b.inherited, "VAR_B should not be inherited");
    assert_eq!(var_b.source_task_id, task.id);
}

#[tokio::test]
async fn test_find_inherited_child_overrides_parent() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;

    // Create hierarchy: grandparent -> parent -> child
    let grandparent = create_test_task(&pool, project.id, "Grandparent", None).await;
    let parent = create_test_task(&pool, project.id, "Parent", Some(grandparent.id)).await;
    let child = create_test_task(&pool, project.id, "Child", Some(parent.id)).await;

    // All three have VAR_A with different values
    let _var_gp = create_variable(&pool, grandparent.id, "VAR_A", "grandparent_value").await;
    let _var_p = create_variable(&pool, parent.id, "VAR_A", "parent_value").await;
    let _var_c = create_variable(&pool, child.id, "VAR_A", "child_value").await;

    // Call find_inherited on child
    let result = TaskVariable::find_inherited(&pool, child.id)
        .await
        .expect("find_inherited failed");

    // Should return only 1 variable (child overrides all)
    assert_eq!(
        result.len(),
        1,
        "Should return 1 variable (child override wins)"
    );

    let var_a = find_var(&result, "VAR_A").expect("VAR_A not found");
    assert_eq!(var_a.value, "child_value", "Child value should win");
    assert!(
        !var_a.inherited,
        "VAR_A should not be inherited (defined on child)"
    );
    assert_eq!(var_a.source_task_id, child.id);
}

#[tokio::test]
async fn test_find_inherited_inherits_from_ancestors() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;

    // Create hierarchy: grandparent -> parent -> child
    let grandparent = create_test_task(&pool, project.id, "Grandparent", None).await;
    let parent = create_test_task(&pool, project.id, "Parent", Some(grandparent.id)).await;
    let child = create_test_task(&pool, project.id, "Child", Some(parent.id)).await;

    // Each level has a unique variable
    let _var_gp = create_variable(&pool, grandparent.id, "VAR_GP", "from_grandparent").await;
    let _var_p = create_variable(&pool, parent.id, "VAR_P", "from_parent").await;
    let _var_c = create_variable(&pool, child.id, "VAR_C", "from_child").await;

    // Call find_inherited on child
    let result = TaskVariable::find_inherited(&pool, child.id)
        .await
        .expect("find_inherited failed");

    // Should return all 3 variables
    assert_eq!(result.len(), 3, "Should return 3 variables");

    // Check each variable
    let var_gp = find_var(&result, "VAR_GP").expect("VAR_GP not found");
    assert_eq!(var_gp.value, "from_grandparent");
    assert!(var_gp.inherited, "VAR_GP should be inherited");
    assert_eq!(var_gp.source_task_id, grandparent.id);

    let var_p = find_var(&result, "VAR_P").expect("VAR_P not found");
    assert_eq!(var_p.value, "from_parent");
    assert!(var_p.inherited, "VAR_P should be inherited");
    assert_eq!(var_p.source_task_id, parent.id);

    let var_c = find_var(&result, "VAR_C").expect("VAR_C not found");
    assert_eq!(var_c.value, "from_child");
    assert!(!var_c.inherited, "VAR_C should not be inherited");
    assert_eq!(var_c.source_task_id, child.id);
}

#[tokio::test]
async fn test_find_inherited_deep_hierarchy() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;

    // Create 10-level deep hierarchy
    let mut tasks: Vec<Task> = Vec::new();
    let mut parent_id: Option<Uuid> = None;

    for level in 0..10 {
        let task =
            create_test_task(&pool, project.id, &format!("Level {}", level), parent_id).await;

        // Each level has a unique variable
        let _var = create_variable(
            &pool,
            task.id,
            &format!("VAR_LEVEL_{}", level),
            &format!("value_from_level_{}", level),
        )
        .await;

        parent_id = Some(task.id);
        tasks.push(task);
    }

    // Call find_inherited on the deepest task (level 9)
    let deepest_task = tasks.last().unwrap();
    let result = TaskVariable::find_inherited(&pool, deepest_task.id)
        .await
        .expect("find_inherited failed");

    // Should return 10 variables
    assert_eq!(
        result.len(),
        10,
        "Should return 10 variables from 10 levels"
    );

    // Verify each variable has correct source_task_id and inherited flag
    for (level, task) in tasks.iter().enumerate() {
        let var_name = format!("VAR_LEVEL_{}", level);
        let var = find_var(&result, &var_name).unwrap_or_else(|| panic!("{} not found", var_name));

        assert_eq!(
            var.value,
            format!("value_from_level_{}", level),
            "{} should have correct value",
            var_name
        );
        assert_eq!(
            var.source_task_id, task.id,
            "{} should have correct source_task_id",
            var_name
        );

        // Only the deepest level (9) should have inherited=false
        if level == 9 {
            assert!(
                !var.inherited,
                "VAR_LEVEL_9 should not be inherited (defined on queried task)"
            );
        } else {
            assert!(var.inherited, "{} should be inherited", var_name);
        }
    }
}

#[tokio::test]
async fn test_find_inherited_empty_no_variables() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;

    // Create a task with no parent and no variables
    let task = create_test_task(&pool, project.id, "Empty Task", None).await;

    // Call find_inherited
    let result = TaskVariable::find_inherited(&pool, task.id)
        .await
        .expect("find_inherited failed");

    // Should return empty vector
    assert!(
        result.is_empty(),
        "Should return empty vector for task with no variables"
    );
}

#[tokio::test]
async fn test_find_inherited_parent_only_variables() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;

    // Create parent with variables, child with no variables
    let parent = create_test_task(&pool, project.id, "Parent", None).await;
    let child = create_test_task(&pool, project.id, "Child", Some(parent.id)).await;

    // Only parent has variables
    let _var_a = create_variable(&pool, parent.id, "VAR_A", "parent_value").await;
    let _var_b = create_variable(&pool, parent.id, "VAR_B", "parent_value_b").await;

    // Call find_inherited on child
    let result = TaskVariable::find_inherited(&pool, child.id)
        .await
        .expect("find_inherited failed");

    // Should return parent's 2 variables, both inherited
    assert_eq!(result.len(), 2, "Should return 2 inherited variables");

    let var_a = find_var(&result, "VAR_A").expect("VAR_A not found");
    assert_eq!(var_a.value, "parent_value");
    assert!(var_a.inherited, "VAR_A should be inherited");
    assert_eq!(var_a.source_task_id, parent.id);

    let var_b = find_var(&result, "VAR_B").expect("VAR_B not found");
    assert_eq!(var_b.value, "parent_value_b");
    assert!(var_b.inherited, "VAR_B should be inherited");
    assert_eq!(var_b.source_task_id, parent.id);
}

#[tokio::test]
async fn test_find_inherited_partial_override_in_chain() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;

    // Create hierarchy: grandparent -> parent -> child
    let grandparent = create_test_task(&pool, project.id, "Grandparent", None).await;
    let parent = create_test_task(&pool, project.id, "Parent", Some(grandparent.id)).await;
    let child = create_test_task(&pool, project.id, "Child", Some(parent.id)).await;

    // Grandparent has VAR_A and VAR_B
    let _var_gp_a = create_variable(&pool, grandparent.id, "VAR_A", "gp_a").await;
    let _var_gp_b = create_variable(&pool, grandparent.id, "VAR_B", "gp_b").await;

    // Parent overrides VAR_A only
    let _var_p_a = create_variable(&pool, parent.id, "VAR_A", "parent_a").await;

    // Child overrides VAR_B only
    let _var_c_b = create_variable(&pool, child.id, "VAR_B", "child_b").await;

    // Call find_inherited on child
    let result = TaskVariable::find_inherited(&pool, child.id)
        .await
        .expect("find_inherited failed");

    // Should return 2 variables
    assert_eq!(result.len(), 2, "Should return 2 variables");

    // VAR_A should come from parent (parent overrides grandparent)
    let var_a = find_var(&result, "VAR_A").expect("VAR_A not found");
    assert_eq!(var_a.value, "parent_a", "VAR_A should come from parent");
    assert!(var_a.inherited, "VAR_A should be inherited (from parent)");
    assert_eq!(var_a.source_task_id, parent.id);

    // VAR_B should come from child (child overrides grandparent's original)
    let var_b = find_var(&result, "VAR_B").expect("VAR_B not found");
    assert_eq!(var_b.value, "child_b", "VAR_B should come from child");
    assert!(
        !var_b.inherited,
        "VAR_B should not be inherited (defined on child)"
    );
    assert_eq!(var_b.source_task_id, child.id);
}

#[tokio::test]
async fn test_find_inherited_results_sorted_by_name() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;

    // Create a task with variables in non-alphabetical order
    let task = create_test_task(&pool, project.id, "Task", None).await;

    // Create variables in non-alphabetical order
    let _var_z = create_variable(&pool, task.id, "ZEBRA", "z_value").await;
    let _var_a = create_variable(&pool, task.id, "ALPHA", "a_value").await;
    let _var_m = create_variable(&pool, task.id, "MIDDLE", "m_value").await;

    // Call find_inherited
    let result = TaskVariable::find_inherited(&pool, task.id)
        .await
        .expect("find_inherited failed");

    // Should return 3 variables sorted by name
    assert_eq!(result.len(), 3, "Should return 3 variables");
    assert_eq!(result[0].name, "ALPHA", "First should be ALPHA");
    assert_eq!(result[1].name, "MIDDLE", "Second should be MIDDLE");
    assert_eq!(result[2].name, "ZEBRA", "Third should be ZEBRA");
}
