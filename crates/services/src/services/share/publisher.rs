//! Publisher for sharing local tasks to the Hive.
//!
//! This module handles pushing local tasks to the Hive server. When a task is shared,
//! the local `tasks.shared_task_id` is set to link it to the Hive task.
//!
//! # Note
//!
//! The local `shared_tasks` table is no longer used. ElectricSQL handles syncing
//! tasks from the Hive back to the local database.

use db::{
    DBService,
    models::{project::Project, task::Task},
};
use remote::routes::tasks::{
    CreateSharedTaskRequest, DeleteSharedTaskRequest, UpdateSharedTaskRequest,
};
use tracing::info;
use uuid::Uuid;

use super::{ShareError, status};
use crate::services::remote_client::{CreateRemoteProjectPayload, RemoteClient};

#[derive(Clone)]
pub struct SharePublisher {
    db: DBService,
    client: RemoteClient,
}

impl SharePublisher {
    pub fn new(db: DBService, client: RemoteClient) -> Self {
        Self { db, client }
    }

    /// Share a task to the Hive.
    ///
    /// The `assignee_user_id` is optional - if provided, the task will be assigned to that user.
    /// If not provided, the task will be shared without an assignee.
    pub async fn share_task(
        &self,
        task_id: Uuid,
        assignee_user_id: Option<Uuid>,
    ) -> Result<Uuid, ShareError> {
        let task = Task::find_by_id(&self.db.pool, task_id)
            .await?
            .ok_or(ShareError::TaskNotFound(task_id))?;

        if task.shared_task_id.is_some() {
            return Err(ShareError::AlreadyShared(task.id));
        }

        let project = Project::find_by_id(&self.db.pool, task.project_id)
            .await?
            .ok_or(ShareError::ProjectNotFound(task.project_id))?;

        // Auto-link the project if it's not already linked
        let remote_project_id = match project.remote_project_id {
            Some(id) => id,
            None => self.ensure_project_linked(&project).await?,
        };

        let payload = CreateSharedTaskRequest {
            project_id: remote_project_id,
            title: task.title.clone(),
            description: task.description.clone(),
            status: Some(status::to_remote(&task.status)),
            assignee_user_id,
            start_attempt: false, // Never auto-dispatch when sharing from local node
            source_task_id: None, // Not a re-sync operation
            source_node_id: None,
        };

        let remote_task = self.client.create_shared_task(&payload).await?;

        // Link the local task to the Hive task
        Task::set_shared_task_id(&self.db.pool, task.id, Some(remote_task.task.id)).await?;

        Ok(remote_task.task.id)
    }

    pub async fn update_shared_task(&self, task: &Task) -> Result<(), ShareError> {
        // early exit if task has not been shared
        let Some(shared_task_id) = task.shared_task_id else {
            return Ok(());
        };

        let payload = UpdateSharedTaskRequest {
            title: Some(task.title.clone()),
            description: task.description.clone(),
            status: Some(status::to_remote(&task.status)),
            // Always sync archived_at - use Some(None) to unarchive, Some(Some(ts)) to archive
            archived_at: Some(task.archived_at),
            version: None,
        };

        self.client
            .update_shared_task(shared_task_id, &payload)
            .await?;

        Ok(())
    }

    pub async fn update_shared_task_by_id(&self, task_id: Uuid) -> Result<(), ShareError> {
        let task = Task::find_by_id(&self.db.pool, task_id)
            .await?
            .ok_or(ShareError::TaskNotFound(task_id))?;

        self.update_shared_task(&task).await
    }

    pub async fn delete_shared_task(&self, shared_task_id: Uuid) -> Result<(), ShareError> {
        // Find the local task that references this shared task
        let local_task = Task::find_by_shared_task_id(&self.db.pool, shared_task_id).await?;

        // Get the version from the task's remote_version field if available
        let payload = DeleteSharedTaskRequest { version: None };

        self.client
            .delete_shared_task(shared_task_id, &payload)
            .await?;

        // Unlink the local task
        if let Some(local_task) = local_task {
            Task::set_shared_task_id(&self.db.pool, local_task.id, None).await?;
        }

        Ok(())
    }

    /// Auto-creates a remote project in the Hive and links it to the local project.
    /// Uses the first available organization (preferring personal org).
    /// Public version for use in startup migration.
    pub async fn ensure_project_linked_public(
        &self,
        project: &Project,
    ) -> Result<Uuid, ShareError> {
        self.ensure_project_linked(project).await
    }

    /// Auto-creates a remote project in the Hive and links it to the local project.
    /// Uses the first available organization (preferring personal org).
    async fn ensure_project_linked(&self, project: &Project) -> Result<Uuid, ShareError> {
        // Get organizations - use the first one (personal org is typically first)
        let orgs = self.client.list_organizations().await?;
        let org = orgs
            .organizations
            .first()
            .ok_or(ShareError::NoOrganizations)?;

        info!(
            project_id = %project.id,
            project_name = %project.name,
            organization_id = %org.id,
            "Auto-creating remote project for Hive sync"
        );

        // Create the remote project
        let remote_project = self
            .client
            .create_project(&CreateRemoteProjectPayload {
                organization_id: org.id,
                name: project.name.clone(),
                metadata: None,
            })
            .await?;

        // Link the local project to the remote project
        Project::set_remote_project_id(&self.db.pool, project.id, Some(remote_project.id)).await?;

        info!(
            project_id = %project.id,
            remote_project_id = %remote_project.id,
            "Auto-linked local project to remote project"
        );

        Ok(remote_project.id)
    }
}
