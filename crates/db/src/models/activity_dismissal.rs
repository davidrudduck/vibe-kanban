//! Activity Dismissal model for tracking dismissed activity feed items.
//!
//! Allows users to dismiss activity feed items and later view/restore them.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

/// Represents a dismissed activity feed item.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ActivityDismissal {
    pub id: Uuid,
    pub task_id: Uuid,
    pub dismissed_at: DateTime<Utc>,
}

impl ActivityDismissal {
    /// Dismiss an activity item for a task.
    /// Uses INSERT OR IGNORE to handle duplicate dismissals gracefully.
    pub async fn dismiss(pool: &SqlitePool, task_id: Uuid) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();
        sqlx::query_as!(
            ActivityDismissal,
            r#"INSERT INTO activity_dismissals (id, task_id)
               VALUES ($1, $2)
               ON CONFLICT(task_id) DO UPDATE SET dismissed_at = dismissed_at
               RETURNING id as "id!: Uuid", task_id as "task_id!: Uuid", dismissed_at as "dismissed_at!: DateTime<Utc>""#,
            id,
            task_id
        )
        .fetch_one(pool)
        .await
    }

    /// Undismiss an activity item (remove from dismissals).
    pub async fn undismiss(pool: &SqlitePool, task_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "DELETE FROM activity_dismissals WHERE task_id = $1",
            task_id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Clear dismissal for a task (used when task status changes).
    /// This is an alias for undismiss, kept for semantic clarity in the task model.
    pub async fn clear_for_task(pool: &SqlitePool, task_id: Uuid) -> Result<(), sqlx::Error> {
        Self::undismiss(pool, task_id).await
    }

    /// Check if a task is dismissed.
    pub async fn is_dismissed(pool: &SqlitePool, task_id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r#"SELECT EXISTS(SELECT 1 FROM activity_dismissals WHERE task_id = $1) as "exists!: bool""#,
            task_id
        )
        .fetch_one(pool)
        .await?;
        Ok(result)
    }
}
