//! Electric Task Sync Service
//!
//! This module provides a service for syncing tasks from the Hive PostgreSQL database
//! to local SQLite via ElectricSQL Shape API.
//!
//! ## Key Features
//!
//! - Initial sync: Fetches all tasks for a project from Electric
//! - Incremental sync: Processes insert/update/delete operations
//! - Handles must-refetch control messages (server restart)
//! - Integrates with existing Task model via `upsert_remote_task()`
//!
//! ## Usage
//!
//! ```ignore
//! let service = ElectricTaskSyncService::new(pool, electric_url);
//! service.sync_project_tasks(project_id, remote_project_id).await?;
//! ```

use db::models::task::{Task, TaskStatus};
use serde::Deserialize;
use sqlx::SqlitePool;
use thiserror::Error;
use uuid::Uuid;

use super::electric_sync::{
    ElectricClient, ElectricError, ShapeConfig, ShapeOperation, ShapeState,
};

/// Errors that can occur during Electric task sync.
#[derive(Debug, Error)]
pub enum ElectricTaskSyncError {
    /// Electric API error.
    #[error("Electric API error: {0}")]
    Electric(#[from] ElectricError),

    /// Database error.
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    /// JSON parsing error.
    #[error("JSON parse error: {0}")]
    JsonParse(#[from] serde_json::Error),

    /// Missing required field in task data.
    #[error("Missing required field: {0}")]
    MissingField(String),

    /// Invalid task status.
    #[error("Invalid task status: {0}")]
    InvalidStatus(String),

    /// Invalid UUID in task data.
    #[error("Invalid UUID: {0}")]
    InvalidUuid(#[from] uuid::Error),
}

/// Task data received from Electric Shape API.
#[derive(Debug, Clone, Deserialize)]
pub struct ElectricTaskData {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub version: i64,
    pub assignee_user_id: Option<String>,
    pub assignee_first_name: Option<String>,
    pub assignee_last_name: Option<String>,
    pub assignee_username: Option<String>,
    pub updated_at: Option<String>,
}

impl ElectricTaskData {
    /// Parse the task ID as a UUID.
    pub fn parse_id(&self) -> Result<Uuid, ElectricTaskSyncError> {
        Ok(Uuid::parse_str(&self.id)?)
    }

    /// Parse the project ID as a UUID.
    pub fn parse_project_id(&self) -> Result<Uuid, ElectricTaskSyncError> {
        Ok(Uuid::parse_str(&self.project_id)?)
    }

    /// Parse the assignee user ID as a UUID.
    pub fn parse_assignee_user_id(&self) -> Result<Option<Uuid>, ElectricTaskSyncError> {
        match &self.assignee_user_id {
            Some(id) => Ok(Some(Uuid::parse_str(id)?)),
            None => Ok(None),
        }
    }

    /// Convert status string to TaskStatus enum.
    pub fn parse_status(&self) -> Result<TaskStatus, ElectricTaskSyncError> {
        match self.status.as_str() {
            "todo" => Ok(TaskStatus::Todo),
            "inprogress" => Ok(TaskStatus::InProgress),
            "inreview" => Ok(TaskStatus::InReview),
            "done" => Ok(TaskStatus::Done),
            "cancelled" => Ok(TaskStatus::Cancelled),
            _ => Err(ElectricTaskSyncError::InvalidStatus(self.status.clone())),
        }
    }

    /// Get the assignee display name.
    pub fn assignee_display_name(&self) -> Option<String> {
        match (&self.assignee_first_name, &self.assignee_last_name) {
            (Some(first), Some(last)) => Some(format!("{} {}", first, last)),
            (Some(first), None) => Some(first.clone()),
            (None, Some(last)) => Some(last.clone()),
            (None, None) => None,
        }
    }
}

/// Result of a sync operation.
#[derive(Debug, Default)]
pub struct SyncResult {
    /// Number of tasks inserted.
    pub inserted: usize,
    /// Number of tasks updated.
    pub updated: usize,
    /// Number of tasks deleted.
    pub deleted: usize,
    /// Whether a refetch was required.
    pub refetched: bool,
}

/// Service for syncing tasks via Electric Shape API.
#[derive(Clone)]
pub struct ElectricTaskSyncService {
    pool: SqlitePool,
    electric_url: String,
}

impl ElectricTaskSyncService {
    /// Create a new Electric task sync service.
    pub fn new(pool: SqlitePool, electric_url: String) -> Self {
        Self { pool, electric_url }
    }

    /// Sync tasks for a project from Electric.
    ///
    /// This performs an initial sync, fetching all tasks for the project
    /// and applying them to the local database.
    ///
    /// # Arguments
    ///
    /// * `local_project_id` - The local project ID to associate tasks with
    /// * `remote_project_id` - The remote project ID to filter tasks by
    ///
    /// # Returns
    ///
    /// A `SyncResult` with counts of inserted, updated, and deleted tasks.
    pub async fn sync_project_tasks(
        &self,
        local_project_id: Uuid,
        remote_project_id: Uuid,
    ) -> Result<SyncResult, ElectricTaskSyncError> {
        let config = ShapeConfig {
            base_url: self.electric_url.clone(),
            table: "shared_tasks".to_string(),
            where_clause: Some(format!(r#""project_id" = '{}'"#, remote_project_id)),
            columns: None,
        };

        let client = ElectricClient::new(config)?;
        let mut state = ShapeState::initial();
        let mut result = SyncResult::default();
        let mut active_task_ids = Vec::new();

        loop {
            let (operations, new_state) = client.fetch(&state, false).await?;
            let mut sync_complete = false;
            let mut needs_refetch = false;

            for op in &operations {
                match op {
                    ShapeOperation::Insert { value, .. } => {
                        let task_data: ElectricTaskData = serde_json::from_value(value.clone())?;
                        let shared_id = task_data.parse_id()?;

                        self.upsert_task(local_project_id, &task_data).await?;
                        active_task_ids.push(shared_id);
                        result.inserted += 1;
                    }
                    ShapeOperation::Update { value, .. } => {
                        let task_data: ElectricTaskData = serde_json::from_value(value.clone())?;
                        let shared_id = task_data.parse_id()?;

                        self.upsert_task(local_project_id, &task_data).await?;
                        active_task_ids.push(shared_id);
                        result.updated += 1;
                    }
                    ShapeOperation::Delete { key } => {
                        if let Some(id) = Self::extract_uuid_from_key(key) {
                            Task::delete_by_shared_task_id(&self.pool, id).await?;
                            result.deleted += 1;
                        }
                    }
                    ShapeOperation::UpToDate => {
                        // Sync is complete
                        sync_complete = true;
                    }
                    ShapeOperation::MustRefetch => {
                        // Server restarted, need to refetch from beginning
                        tracing::info!(
                            project_id = %remote_project_id,
                            "Electric server requires refetch, restarting sync"
                        );
                        needs_refetch = true;
                    }
                }
            }

            if needs_refetch {
                state = ShapeState::initial();
                active_task_ids.clear();
                result = SyncResult {
                    refetched: true,
                    ..Default::default()
                };
                continue;
            }

            // Check if we've reached end of initial sync
            if sync_complete {
                break;
            }

            state = new_state;
        }

        // Clean up stale remote tasks for this project
        if !active_task_ids.is_empty() {
            Task::delete_stale_remote_tasks(&self.pool, local_project_id, &active_task_ids).await?;
        }

        tracing::info!(
            project_id = %remote_project_id,
            inserted = result.inserted,
            updated = result.updated,
            deleted = result.deleted,
            refetched = result.refetched,
            "Completed Electric task sync"
        );

        Ok(result)
    }

    /// Upsert a task from Electric data.
    async fn upsert_task(
        &self,
        local_project_id: Uuid,
        task_data: &ElectricTaskData,
    ) -> Result<Task, ElectricTaskSyncError> {
        let shared_id = task_data.parse_id()?;
        let status = task_data.parse_status()?;
        let assignee_user_id = task_data.parse_assignee_user_id()?;
        let assignee_name = task_data.assignee_display_name();

        let task = Task::upsert_remote_task(
            &self.pool,
            Uuid::new_v4(),
            local_project_id,
            shared_id,
            task_data.title.clone(),
            task_data.description.clone(),
            status,
            assignee_user_id,
            assignee_name,
            task_data.assignee_username.clone(),
            task_data.version,
            None, // activity_at - could parse from updated_at
        )
        .await?;

        Ok(task)
    }

    /// Extract UUID from an Electric shape key.
    ///
    /// Handles both simple UUID keys and complex keys like `"schema"."table"/"uuid"`.
    fn extract_uuid_from_key(key: &str) -> Option<Uuid> {
        // Try parsing as a simple UUID first
        if let Ok(uuid) = Uuid::parse_str(key) {
            return Some(uuid);
        }

        // Extract from quoted format: ..."uuid"
        key.rsplit('/')
            .next()
            .and_then(|s| s.trim_matches('"').parse().ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_uuid_from_simple_key() {
        let uuid = Uuid::new_v4();
        let key = uuid.to_string();

        let extracted = ElectricTaskSyncService::extract_uuid_from_key(&key);
        assert_eq!(extracted, Some(uuid));
    }

    #[test]
    fn test_extract_uuid_from_complex_key() {
        let uuid = Uuid::new_v4();
        let key = format!(r#""public"."shared_tasks"/"{uuid}""#);

        let extracted = ElectricTaskSyncService::extract_uuid_from_key(&key);
        assert_eq!(extracted, Some(uuid));
    }

    #[test]
    fn test_extract_uuid_from_invalid_key() {
        let key = "not-a-uuid";
        let extracted = ElectricTaskSyncService::extract_uuid_from_key(key);
        assert!(extracted.is_none());
    }

    #[test]
    fn test_parse_task_status() {
        let task = ElectricTaskData {
            id: Uuid::new_v4().to_string(),
            project_id: Uuid::new_v4().to_string(),
            title: "Test".to_string(),
            description: None,
            status: "inprogress".to_string(),
            version: 1,
            assignee_user_id: None,
            assignee_first_name: None,
            assignee_last_name: None,
            assignee_username: None,
            updated_at: None,
        };

        assert_eq!(task.parse_status().unwrap(), TaskStatus::InProgress);
    }

    #[test]
    fn test_parse_assignee_display_name() {
        let task = ElectricTaskData {
            id: Uuid::new_v4().to_string(),
            project_id: Uuid::new_v4().to_string(),
            title: "Test".to_string(),
            description: None,
            status: "todo".to_string(),
            version: 1,
            assignee_user_id: None,
            assignee_first_name: Some("John".to_string()),
            assignee_last_name: Some("Doe".to_string()),
            assignee_username: None,
            updated_at: None,
        };

        assert_eq!(task.assignee_display_name(), Some("John Doe".to_string()));
    }

    #[test]
    fn test_sync_result_default() {
        let result = SyncResult::default();
        assert_eq!(result.inserted, 0);
        assert_eq!(result.updated, 0);
        assert_eq!(result.deleted, 0);
        assert!(!result.refetched);
    }
}
