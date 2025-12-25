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
}
