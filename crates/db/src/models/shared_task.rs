//! Shared task model for caching remote tasks locally (legacy implementation).
//!
//! # DEPRECATION NOTICE
//!
//! This module is **DEPRECATED** and its database tables have been **DROPPED**.
//! The shared_tasks and shared_activity_cursors tables no longer exist.
//!
//! The type definitions are kept for backwards compatibility with existing code
//! that still references these types. All database operations will return errors.
//!
//! ## Migration
//!
//! This module was removed as part of the explicit swarm linking migration.
//! Tasks are now synced via the swarm using the swarm_task_id field on tasks.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Executor, Sqlite, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

use super::task::TaskStatus;

/// Shared task from the hive (legacy - tables dropped).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SharedTask {
    pub id: Uuid,
    pub swarm_project_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub assignee_user_id: Option<Uuid>,
    pub assignee_first_name: Option<String>,
    pub assignee_last_name: Option<String>,
    pub assignee_username: Option<String>,
    pub version: i64,
    pub last_event_seq: Option<i64>,
    #[ts(type = "Date")]
    pub created_at: DateTime<Utc>,
    #[ts(type = "Date")]
    pub updated_at: DateTime<Utc>,
    #[ts(type = "Date | null")]
    pub activity_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct SharedTaskInput {
    pub id: Uuid,
    pub swarm_project_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub assignee_user_id: Option<Uuid>,
    pub assignee_first_name: Option<String>,
    pub assignee_last_name: Option<String>,
    pub assignee_username: Option<String>,
    pub version: i64,
    pub last_event_seq: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub activity_at: Option<DateTime<Utc>>,
}

fn table_dropped_error() -> sqlx::Error {
    sqlx::Error::Protocol(
        "shared_tasks table has been dropped - use swarm sync instead".to_string(),
    )
}

impl SharedTask {
    /// DEPRECATED: Table has been dropped.
    pub async fn list_by_swarm_project_id(
        _pool: &SqlitePool,
        _swarm_project_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        Err(table_dropped_error())
    }

    /// DEPRECATED: Table has been dropped.
    pub async fn upsert<'e, E>(_executor: E, _data: SharedTaskInput) -> Result<Self, sqlx::Error>
    where
        E: Executor<'e, Database = Sqlite>,
    {
        Err(table_dropped_error())
    }

    /// DEPRECATED: Table has been dropped.
    pub async fn find_by_id(_pool: &SqlitePool, _id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        Err(table_dropped_error())
    }

    /// DEPRECATED: Table has been dropped.
    pub async fn remove<'e, E>(_executor: E, _id: Uuid) -> Result<(), sqlx::Error>
    where
        E: Executor<'e, Database = Sqlite>,
    {
        Err(table_dropped_error())
    }

    /// DEPRECATED: Table has been dropped.
    pub async fn remove_many<'e, E>(_executor: E, _ids: &[Uuid]) -> Result<(), sqlx::Error>
    where
        E: Executor<'e, Database = Sqlite>,
    {
        Err(table_dropped_error())
    }

    /// DEPRECATED: Table has been dropped.
    pub async fn find_by_rowid(
        _pool: &SqlitePool,
        _rowid: i64,
    ) -> Result<Option<Self>, sqlx::Error> {
        Err(table_dropped_error())
    }

    /// DEPRECATED: Table has been dropped.
    pub async fn find_unassigned(_pool: &SqlitePool) -> Result<Vec<Self>, sqlx::Error> {
        Err(table_dropped_error())
    }
}

#[derive(Debug, Clone)]
pub struct SharedActivityCursor {
    pub swarm_project_id: Uuid,
    pub last_seq: i64,
    pub updated_at: DateTime<Utc>,
}

impl SharedActivityCursor {
    /// DEPRECATED: Table has been dropped.
    pub async fn get(
        _pool: &SqlitePool,
        _swarm_project_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        Err(table_dropped_error())
    }

    /// DEPRECATED: Table has been dropped.
    pub async fn upsert<'e, E>(
        _executor: E,
        _swarm_project_id: Uuid,
        _last_seq: i64,
    ) -> Result<Self, sqlx::Error>
    where
        E: Executor<'e, Database = Sqlite>,
    {
        Err(table_dropped_error())
    }
}
