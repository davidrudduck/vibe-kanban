use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

/// Names of system-provided variables that are automatically available
pub const SYSTEM_VARIABLE_NAMES: &[&str] = &[
    "TASK_ID",
    "PARENT_TASK_ID",
    "TASK_TITLE",
    "TASK_DESCRIPTION",
    "TASK_LABEL",
    "PROJECT_ID",
    "PROJECT_TITLE",
    "IS_SUBTASK",
];

/// Generate system variables for a task.
/// These are computed at runtime and not stored in the database.
pub async fn get_system_variables(
    pool: &SqlitePool,
    task_id: Uuid,
) -> Result<Vec<ResolvedVariable>, sqlx::Error> {
    use crate::models::label::Label;
    use crate::models::project::Project;
    use crate::models::task::Task;

    let task = Task::find_by_id(pool, task_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;

    let project = Project::find_by_id(pool, task.project_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;

    let labels = Label::find_by_task_id(pool, task_id).await?;
    let label_name = labels.first().map(|l| l.name.clone()).unwrap_or_default();

    Ok(vec![
        ResolvedVariable {
            name: "TASK_ID".to_string(),
            value: task.id.to_string(),
            source_task_id: task_id,
            inherited: false,
        },
        ResolvedVariable {
            name: "PARENT_TASK_ID".to_string(),
            value: task.parent_task_id.map(|id| id.to_string()).unwrap_or_default(),
            source_task_id: task_id,
            inherited: false,
        },
        ResolvedVariable {
            name: "TASK_TITLE".to_string(),
            value: task.title.clone(),
            source_task_id: task_id,
            inherited: false,
        },
        ResolvedVariable {
            name: "TASK_DESCRIPTION".to_string(),
            value: task.description.clone().unwrap_or_default(),
            source_task_id: task_id,
            inherited: false,
        },
        ResolvedVariable {
            name: "TASK_LABEL".to_string(),
            value: label_name,
            source_task_id: task_id,
            inherited: false,
        },
        ResolvedVariable {
            name: "PROJECT_ID".to_string(),
            value: project.id.to_string(),
            source_task_id: task_id,
            inherited: false,
        },
        ResolvedVariable {
            name: "PROJECT_TITLE".to_string(),
            value: project.name.clone(),
            source_task_id: task_id,
            inherited: false,
        },
        ResolvedVariable {
            name: "IS_SUBTASK".to_string(),
            value: if task.parent_task_id.is_some() { "true" } else { "false" }.to_string(),
            source_task_id: task_id,
            inherited: false,
        },
    ])
}

/// A variable defined on a task, used for $VAR expansion in task descriptions
#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct TaskVariable {
    pub id: Uuid,
    pub task_id: Uuid,
    pub name: String,
    pub value: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request to create a new task variable
#[derive(Debug, Deserialize, TS)]
pub struct CreateTaskVariable {
    pub name: String,
    pub value: String,
}

/// Request to update an existing task variable
#[derive(Debug, Deserialize, TS)]
pub struct UpdateTaskVariable {
    pub name: Option<String>,
    pub value: Option<String>,
}

/// A resolved variable with source information for inheritance display
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ResolvedVariable {
    pub name: String,
    pub value: String,
    /// The task ID this variable was defined on (may differ from requested task_id for inherited vars)
    pub source_task_id: Uuid,
    /// True if this variable was inherited from a parent task
    pub inherited: bool,
}

impl TaskVariable {
    /// Find all variables defined directly on a task (not inherited)
    pub async fn find_by_task_id(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            TaskVariable,
            r#"SELECT
                id as "id!: Uuid",
                task_id as "task_id!: Uuid",
                name,
                value,
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>"
               FROM task_variables
               WHERE task_id = $1
               ORDER BY name ASC"#,
            task_id
        )
        .fetch_all(pool)
        .await
    }

    /// Find a variable by its ID
    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            TaskVariable,
            r#"SELECT
                id as "id!: Uuid",
                task_id as "task_id!: Uuid",
                name,
                value,
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>"
               FROM task_variables
               WHERE id = $1"#,
            id
        )
        .fetch_optional(pool)
        .await
    }

    /// Create a new variable on a task
    pub async fn create(
        pool: &SqlitePool,
        task_id: Uuid,
        data: &CreateTaskVariable,
    ) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();
        sqlx::query_as!(
            TaskVariable,
            r#"INSERT INTO task_variables (id, task_id, name, value)
               VALUES ($1, $2, $3, $4)
               RETURNING
                id as "id!: Uuid",
                task_id as "task_id!: Uuid",
                name,
                value,
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>""#,
            id,
            task_id,
            data.name,
            data.value
        )
        .fetch_one(pool)
        .await
    }

    /// Update an existing variable
    pub async fn update(
        pool: &SqlitePool,
        id: Uuid,
        data: &UpdateTaskVariable,
    ) -> Result<Self, sqlx::Error> {
        let existing = Self::find_by_id(pool, id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;

        let name = data.name.as_ref().unwrap_or(&existing.name);
        let value = data.value.as_ref().unwrap_or(&existing.value);

        sqlx::query_as!(
            TaskVariable,
            r#"UPDATE task_variables
               SET name = $2, value = $3, updated_at = datetime('now', 'subsec')
               WHERE id = $1
               RETURNING
                id as "id!: Uuid",
                task_id as "task_id!: Uuid",
                name,
                value,
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>""#,
            id,
            name,
            value
        )
        .fetch_one(pool)
        .await
    }

    /// Delete a variable
    pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!("DELETE FROM task_variables WHERE id = $1", id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// Find all variables for a task including inherited ones from parent chain.
    /// Child variables override parent variables with the same name.
    /// Returns variables as a list with source information.
    ///
    /// Performance: Uses a recursive CTE to traverse the parent chain and fetch
    /// all variables in a single query, reducing from O(2*depth) queries to O(1).
    pub async fn find_inherited(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<Vec<ResolvedVariable>, sqlx::Error> {
        // Use recursive CTE to traverse parent chain and collect variables in one query.
        // The CTE builds the task chain with depth, then joins variables.
        // We use ROW_NUMBER partitioned by name and ordered by depth ASC to get
        // the closest (child) variable for each name, allowing child overrides.
        let rows = sqlx::query!(
            r#"
            WITH RECURSIVE task_chain AS (
                -- Base case: start with the requested task at depth 0
                SELECT id, parent_task_id, CAST(0 AS INTEGER) as depth
                FROM tasks
                WHERE id = $1

                UNION ALL

                -- Recursive case: traverse to parent, incrementing depth
                SELECT t.id, t.parent_task_id, CAST(tc.depth + 1 AS INTEGER)
                FROM tasks t
                INNER JOIN task_chain tc ON t.id = tc.parent_task_id
            ),
            -- Join variables with task chain and rank by depth per variable name
            ranked_vars AS (
                SELECT
                    tv.name,
                    tv.value,
                    tc.id as source_task_id,
                    tc.depth,
                    ROW_NUMBER() OVER (PARTITION BY tv.name ORDER BY tc.depth ASC) as rn
                FROM task_chain tc
                INNER JOIN task_variables tv ON tv.task_id = tc.id
            )
            -- Select only the closest (lowest depth) variable for each name
            SELECT
                name as "name!",
                value as "value!",
                source_task_id as "source_task_id!: Uuid",
                depth as "depth!: i32"
            FROM ranked_vars
            WHERE rn = 1
            ORDER BY name ASC
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        // Convert to ResolvedVariable, marking inherited based on depth
        let result = rows
            .into_iter()
            .map(|row| ResolvedVariable {
                name: row.name,
                value: row.value,
                source_task_id: row.source_task_id,
                inherited: row.depth > 0,
            })
            .collect();

        Ok(result)
    }

    /// Get resolved variables as a HashMap suitable for variable expansion
    pub async fn get_variable_map(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<std::collections::HashMap<String, (String, Uuid)>, sqlx::Error> {
        let resolved = Self::find_inherited(pool, task_id).await?;
        Ok(resolved
            .into_iter()
            .map(|rv| (rv.name, (rv.value, rv.source_task_id)))
            .collect())
    }

    /// Find all variables for a task including inherited ones AND system variables.
    /// System variables override user-defined variables with the same name.
    pub async fn find_inherited_with_system(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<Vec<ResolvedVariable>, sqlx::Error> {
        let user_vars = Self::find_inherited(pool, task_id).await?;
        let system_vars = get_system_variables(pool, task_id).await?;

        let system_names: std::collections::HashSet<&str> =
            system_vars.iter().map(|v| v.name.as_str()).collect();

        let mut result: Vec<ResolvedVariable> = user_vars
            .into_iter()
            .filter(|v| !system_names.contains(v.name.as_str()))
            .collect();

        result.extend(system_vars);
        result.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(result)
    }

    /// Get resolved variables including system variables as a HashMap
    /// suitable for variable expansion.
    pub async fn get_variable_map_with_system(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<std::collections::HashMap<String, (String, Uuid)>, sqlx::Error> {
        let resolved = Self::find_inherited_with_system(pool, task_id).await?;
        Ok(resolved
            .into_iter()
            .map(|rv| (rv.name, (rv.value, rv.source_task_id)))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::label::{CreateLabel, Label};
    use crate::models::project::{CreateProject, Project};
    use crate::models::task::{CreateTask, Task};
    use crate::test_utils::create_test_pool;

    #[tokio::test]
    async fn test_system_variables_basic() {
        let (pool, _temp_dir) = create_test_pool().await;

        // Create project
        let project_id = Uuid::new_v4();
        let project = Project::create(
            &pool,
            &CreateProject {
                name: "Test Project".to_string(),
                git_repo_path: "/tmp/test".to_string(),
                use_existing_repo: false,
                clone_url: None,
                setup_script: None,
                dev_script: None,
                cleanup_script: None,
                copy_files: None,
            },
            project_id,
        )
        .await
        .unwrap();

        // Create task
        let task_id = Uuid::new_v4();
        let task = Task::create(
            &pool,
            &CreateTask {
                project_id: project.id,
                title: "Test Task".to_string(),
                description: Some("Test description".to_string()),
                status: None,
                parent_task_id: None,
                image_ids: None,
                shared_task_id: None,
            },
            task_id,
        )
        .await
        .unwrap();

        // Get system variables
        let vars = get_system_variables(&pool, task.id).await.unwrap();

        // Should have exactly 8 variables
        assert_eq!(vars.len(), 8);

        // Verify each variable
        let vars_map: std::collections::HashMap<_, _> =
            vars.iter().map(|v| (v.name.as_str(), &v.value)).collect();

        assert_eq!(vars_map["TASK_ID"], &task.id.to_string());
        assert_eq!(vars_map["PARENT_TASK_ID"], ""); // No parent
        assert_eq!(vars_map["TASK_TITLE"], "Test Task");
        assert_eq!(vars_map["TASK_DESCRIPTION"], "Test description");
        assert_eq!(vars_map["TASK_LABEL"], ""); // No label
        assert_eq!(vars_map["PROJECT_ID"], &project.id.to_string());
        assert_eq!(vars_map["PROJECT_TITLE"], "Test Project");
        assert_eq!(vars_map["IS_SUBTASK"], "false"); // No parent
    }

    #[tokio::test]
    async fn test_system_variables_with_parent_task() {
        let (pool, _temp_dir) = create_test_pool().await;

        let project_id = Uuid::new_v4();
        let project = Project::create(
            &pool,
            &CreateProject {
                name: "Test Project".to_string(),
                git_repo_path: "/tmp/test".to_string(),
                use_existing_repo: false,
                clone_url: None,
                setup_script: None,
                dev_script: None,
                cleanup_script: None,
                copy_files: None,
            },
            project_id,
        )
        .await
        .unwrap();

        // Create parent task
        let parent_id = Uuid::new_v4();
        let parent = Task::create(
            &pool,
            &CreateTask {
                project_id: project.id,
                title: "Parent".to_string(),
                description: None,
                status: None,
                parent_task_id: None,
                image_ids: None,
                shared_task_id: None,
            },
            parent_id,
        )
        .await
        .unwrap();

        // Create child task
        let child_id = Uuid::new_v4();
        let child = Task::create(
            &pool,
            &CreateTask {
                project_id: project.id,
                title: "Child".to_string(),
                description: None,
                status: None,
                parent_task_id: Some(parent.id),
                image_ids: None,
                shared_task_id: None,
            },
            child_id,
        )
        .await
        .unwrap();

        let vars = get_system_variables(&pool, child.id).await.unwrap();
        let vars_map: std::collections::HashMap<_, _> =
            vars.iter().map(|v| (v.name.as_str(), &v.value)).collect();

        assert_eq!(vars_map["PARENT_TASK_ID"], &parent.id.to_string());
        assert_eq!(vars_map["IS_SUBTASK"], "true");
    }

    #[tokio::test]
    async fn test_system_variables_with_label() {
        let (pool, _temp_dir) = create_test_pool().await;

        let project_id = Uuid::new_v4();
        let project = Project::create(
            &pool,
            &CreateProject {
                name: "Test Project".to_string(),
                git_repo_path: "/tmp/test".to_string(),
                use_existing_repo: false,
                clone_url: None,
                setup_script: None,
                dev_script: None,
                cleanup_script: None,
                copy_files: None,
            },
            project_id,
        )
        .await
        .unwrap();

        let task_id = Uuid::new_v4();
        let task = Task::create(
            &pool,
            &CreateTask {
                project_id: project.id,
                title: "Test Task".to_string(),
                description: None,
                status: None,
                parent_task_id: None,
                image_ids: None,
                shared_task_id: None,
            },
            task_id,
        )
        .await
        .unwrap();

        // Create and attach label
        let label = Label::create(
            &pool,
            &CreateLabel {
                project_id: Some(project.id),
                name: "bug".to_string(),
                icon: "🐛".to_string(),
                color: "#ff0000".to_string(),
            },
        )
        .await
        .unwrap();

        Label::attach_to_task(&pool, task.id, label.id)
            .await
            .unwrap();

        let vars = get_system_variables(&pool, task.id).await.unwrap();
        let vars_map: std::collections::HashMap<_, _> =
            vars.iter().map(|v| (v.name.as_str(), &v.value)).collect();

        assert_eq!(vars_map["TASK_LABEL"], "bug");
    }

    #[tokio::test]
    async fn test_system_variables_override_user_defined() {
        let (pool, _temp_dir) = create_test_pool().await;

        let project_id = Uuid::new_v4();
        let project = Project::create(
            &pool,
            &CreateProject {
                name: "Test Project".to_string(),
                git_repo_path: "/tmp/test".to_string(),
                use_existing_repo: false,
                clone_url: None,
                setup_script: None,
                dev_script: None,
                cleanup_script: None,
                copy_files: None,
            },
            project_id,
        )
        .await
        .unwrap();

        let task_id = Uuid::new_v4();
        let task = Task::create(
            &pool,
            &CreateTask {
                project_id: project.id,
                title: "Test Task".to_string(),
                description: None,
                status: None,
                parent_task_id: None,
                image_ids: None,
                shared_task_id: None,
            },
            task_id,
        )
        .await
        .unwrap();

        // Create user variable with same name as system variable
        TaskVariable::create(
            &pool,
            task.id,
            &CreateTaskVariable {
                name: "TASK_ID".to_string(),
                value: "wrong-value".to_string(),
            },
        )
        .await
        .unwrap();

        // Get combined variables
        let vars = TaskVariable::find_inherited_with_system(&pool, task.id)
            .await
            .unwrap();

        // Find TASK_ID variable
        let task_id_var = vars.iter().find(|v| v.name == "TASK_ID").unwrap();

        // Should be the system value, not user value
        assert_eq!(task_id_var.value, task.id.to_string());
        assert_ne!(task_id_var.value, "wrong-value");
    }
}
