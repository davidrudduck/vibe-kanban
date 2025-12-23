use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

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
    pub async fn find_inherited(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<Vec<ResolvedVariable>, sqlx::Error> {
        // Collect all task IDs in the parent chain (starting from current task)
        let mut task_chain: Vec<Uuid> = Vec::new();
        let mut current_task_id: Option<Uuid> = Some(task_id);

        while let Some(tid) = current_task_id {
            task_chain.push(tid);

            // Get parent_task_id for current task
            let parent = sqlx::query_scalar!(
                r#"SELECT parent_task_id as "parent_task_id: Uuid" FROM tasks WHERE id = $1"#,
                tid
            )
            .fetch_optional(pool)
            .await?
            .flatten();

            current_task_id = parent;
        }

        // Collect all variables from all tasks in the chain
        // Start from the root (end of chain) and work towards current task
        // This way, child variables naturally override parent variables
        let mut resolved: std::collections::HashMap<String, ResolvedVariable> =
            std::collections::HashMap::new();

        for (depth, &ancestor_task_id) in task_chain.iter().rev().enumerate() {
            let vars = Self::find_by_task_id(pool, ancestor_task_id).await?;
            let is_current_task = depth == task_chain.len() - 1;

            for var in vars {
                resolved.insert(
                    var.name.clone(),
                    ResolvedVariable {
                        name: var.name,
                        value: var.value,
                        source_task_id: ancestor_task_id,
                        inherited: !is_current_task,
                    },
                );
            }
        }

        // Convert to sorted vector
        let mut result: Vec<ResolvedVariable> = resolved.into_values().collect();
        result.sort_by(|a, b| a.name.cmp(&b.name));
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
}
