//! Core CRUD handlers: get_tasks, get_task, create_task, update_task, delete_task, create_task_and_start.

use std::path::PathBuf;

use axum::{
    Extension, Json,
    extract::{Query, State},
    http::StatusCode,
    response::Json as ResponseJson,
};
use db::models::{
    image::TaskImage,
    project::Project,
    task::{CreateTask, Task, TaskWithAttemptStatus, UpdateTask},
    task_attempt::{CreateTaskAttempt, TaskAttempt, TaskAttemptError},
};
use deployment::Deployment;
use services::services::{
    container::ContainerService,
    worktree_manager::{WorktreeCleanup, WorktreeManager},
};
use sqlx::Error as SqlxError;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};
use crate::routes::tasks::types::{CreateAndStartTaskRequest, TaskQuery};
use super::remote::{create_remote_task, delete_remote_task, update_remote_task};

// ============================================================================
// List and Get
// ============================================================================

pub async fn get_tasks(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<TaskQuery>,
) -> Result<ResponseJson<ApiResponse<Vec<TaskWithAttemptStatus>>>, ApiError> {
    let tasks = Task::find_by_project_id_with_attempt_status(
        &deployment.db().pool,
        query.project_id,
        query.include_archived,
    )
    .await?;

    Ok(ResponseJson(ApiResponse::success(tasks)))
}

pub async fn get_task(
    Extension(task): Extension<Task>,
    State(_deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Task>>, ApiError> {
    Ok(ResponseJson(ApiResponse::success(task)))
}

// ============================================================================
// Create
// ============================================================================

pub async fn create_task(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateTask>,
) -> Result<ResponseJson<ApiResponse<Task>>, ApiError> {
    let pool = &deployment.db().pool;

    // Check if project is remote
    let project = Project::find_by_id(pool, payload.project_id)
        .await?
        .ok_or(ApiError::Database(SqlxError::RowNotFound))?;

    if project.is_remote {
        // Remote project: proxy to Hive and sync locally
        return create_remote_task(&deployment, &project, &payload).await;
    }

    // Local project: existing logic

    // Validate: If creating a subtask, check that the parent doesn't use a shared worktree
    if let Some(parent_task_id) = payload.parent_task_id {
        let parent_task = Task::find_by_id(pool, parent_task_id)
            .await?
            .ok_or(ApiError::Database(SqlxError::RowNotFound))?;

        // Check if parent task uses a shared worktree from its own parent (grandparent)
        if let Some(grandparent_id) = parent_task.parent_task_id {
            let uses_shared =
                TaskAttempt::task_uses_shared_worktree(pool, parent_task_id, grandparent_id)
                    .await
                    .map_err(|e| ApiError::TaskAttempt(TaskAttemptError::Database(e)))?;

            if uses_shared {
                return Err(ApiError::BadRequest(
                    "Cannot create subtask: parent task uses a shared worktree".to_string(),
                ));
            }
        }

        // Auto-unarchive the parent task when a subtask is created
        if Task::unarchive_if_archived(pool, parent_task_id).await? {
            tracing::info!(
                parent_task_id = %parent_task_id,
                "Auto-unarchived parent task due to subtask creation"
            );

            // Sync unarchive to Hive if parent is shared
            if parent_task.shared_task_id.is_some()
                && let Ok(publisher) = deployment.share_publisher()
                && let Some(updated_parent) = Task::find_by_id(pool, parent_task_id).await?
            {
                let publisher = publisher.clone();
                tokio::spawn(async move {
                    if let Err(e) = publisher.update_shared_task(&updated_parent).await {
                        tracing::warn!(?e, "failed to sync parent task unarchive to Hive");
                    }
                });
            }
        }
    }

    let id = Uuid::new_v4();

    tracing::debug!(
        "Creating task '{}' in project {}",
        payload.title,
        payload.project_id
    );

    let task = Task::create(pool, &payload, id).await?;

    if let Some(image_ids) = &payload.image_ids {
        TaskImage::associate_many_dedup(pool, task.id, image_ids).await?;
    }

    // Auto-share task to Hive (will auto-link project if not already linked)
    let mut task = task;
    if let Ok(publisher) = deployment.share_publisher() {
        // Get user_id for sharing - use cached profile if available (optional)
        let user_id = deployment
            .auth_context()
            .cached_profile()
            .await
            .map(|p| p.user_id);

        match publisher.share_task(task.id, user_id).await {
            Ok(shared_task_id) => {
                tracing::info!(
                    task_id = %task.id,
                    shared_task_id = %shared_task_id,
                    "Auto-shared task to Hive"
                );
                // Update local task with shared_task_id for consistency
                if let Some(updated) = Task::find_by_id(pool, task.id).await? {
                    task = updated;
                }
            }
            Err(e) => {
                tracing::warn!(
                    task_id = %task.id,
                    error = ?e,
                    "Failed to auto-share task to Hive"
                );
            }
        }
    }

    Ok(ResponseJson(ApiResponse::success(task)))
}

// ============================================================================
// Create and Start
// ============================================================================

pub async fn create_task_and_start(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateAndStartTaskRequest>,
) -> Result<ResponseJson<ApiResponse<TaskWithAttemptStatus>>, ApiError> {
    let pool = &deployment.db().pool;

    // Check if project is remote - cannot start task attempts on remote projects
    let project = Project::find_by_id(pool, payload.task.project_id)
        .await?
        .ok_or(ApiError::Database(SqlxError::RowNotFound))?;

    if project.is_remote {
        return Err(ApiError::BadRequest(
            "Cannot start task attempts on remote projects. Tasks execute on their origin node."
                .to_string(),
        ));
    }

    let task_id = Uuid::new_v4();
    let task = Task::create(pool, &payload.task, task_id).await?;

    if let Some(image_ids) = &payload.task.image_ids {
        TaskImage::associate_many(pool, task.id, image_ids).await?;
    }

    // Auto-share task if project is linked to the Hive
    if let Some(project) = Project::find_by_id(pool, task.project_id).await?
        && project.remote_project_id.is_some()
        && let Ok(publisher) = deployment.share_publisher()
    {
        // Get user_id for sharing - use cached profile if available (optional)
        let user_id = deployment
            .auth_context()
            .cached_profile()
            .await
            .map(|p| p.user_id);

        match publisher.share_task(task.id, user_id).await {
            Ok(shared_task_id) => {
                tracing::info!(
                    task_id = %task.id,
                    shared_task_id = %shared_task_id,
                    "Auto-shared task to Hive"
                );
            }
            Err(e) => {
                tracing::warn!(
                    task_id = %task.id,
                    error = ?e,
                    "Failed to auto-share task to Hive"
                );
            }
        }
    }

    let attempt_id = Uuid::new_v4();

    // Determine branch name and parent worktree info based on use_parent_worktree flag
    let (git_branch_name, parent_container_ref) = if payload.use_parent_worktree.unwrap_or(false) {
        // Validate task has parent
        let parent_task_id = payload.task.parent_task_id.ok_or_else(|| {
            ApiError::BadRequest("Cannot use parent worktree: task has no parent_task_id".into())
        })?;

        // Get parent task's latest attempt
        let parent_attempts = TaskAttempt::fetch_all(pool, Some(parent_task_id)).await?;
        let parent_attempt = parent_attempts.first().ok_or_else(|| {
            ApiError::BadRequest("Cannot use parent worktree: parent task has no attempts".into())
        })?;

        // Validate parent has a worktree
        let container_ref = parent_attempt.container_ref.clone().ok_or_else(|| {
            ApiError::BadRequest(
                "Cannot use parent worktree: parent attempt has no worktree".into(),
            )
        })?;

        // Validate parent worktree not deleted
        if parent_attempt.worktree_deleted {
            return Err(ApiError::BadRequest(
                "Cannot use parent worktree: parent worktree was deleted".into(),
            ));
        }

        (parent_attempt.branch.clone(), Some(container_ref))
    } else {
        let branch = deployment
            .container()
            .git_branch_from_task_attempt(&attempt_id, &task.title)
            .await;
        (branch, None)
    };

    let task_attempt = TaskAttempt::create(
        pool,
        &CreateTaskAttempt {
            executor: payload.executor_profile_id.executor,
            base_branch: payload.base_branch.clone(),
            branch: git_branch_name,
        },
        attempt_id,
        task.id,
    )
    .await?;

    // If using parent worktree, update container_ref directly and skip worktree creation
    let skip_worktree_creation = if let Some(container_ref) = parent_container_ref {
        TaskAttempt::update_container_ref(pool, task_attempt.id, &container_ref).await?;
        true
    } else {
        false
    };

    let is_attempt_running = deployment
        .container()
        .start_attempt(
            &task_attempt,
            payload.executor_profile_id.clone(),
            skip_worktree_creation,
        )
        .await
        .inspect_err(|err| tracing::error!("Failed to start task attempt: {}", err))
        .is_ok();

    let task = Task::find_by_id(pool, task.id)
        .await?
        .ok_or(ApiError::Database(SqlxError::RowNotFound))?;

    tracing::info!("Started attempt for task {}", task.id);
    Ok(ResponseJson(ApiResponse::success(TaskWithAttemptStatus {
        task,
        has_in_progress_attempt: is_attempt_running,
        has_merged_attempt: false,
        last_attempt_failed: false,
        executor: task_attempt.executor,
        latest_execution_started_at: None, // Will be populated after first execution
        latest_execution_completed_at: None,
    })))
}

// ============================================================================
// Update
// ============================================================================

pub async fn update_task(
    Extension(existing_task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<UpdateTask>,
) -> Result<ResponseJson<ApiResponse<Task>>, ApiError> {
    // Check if this is a task synced from Hive (has shared_task_id)
    if existing_task.shared_task_id.is_some() {
        return update_remote_task(&deployment, &existing_task, &payload).await;
    }

    // Local task: existing logic
    // Use existing values if not provided in update
    let title = payload.title.unwrap_or(existing_task.title);
    let description = match payload.description {
        Some(s) if s.trim().is_empty() => None, // Empty string = clear description
        Some(s) => Some(s),                     // Non-empty string = update description
        None => existing_task.description,      // Field omitted = keep existing
    };
    let status = payload.status.unwrap_or(existing_task.status);
    let parent_task_id = payload.parent_task_id.or(existing_task.parent_task_id);

    let task = Task::update(
        &deployment.db().pool,
        existing_task.id,
        existing_task.project_id,
        title,
        description,
        status,
        parent_task_id,
    )
    .await?;

    // Auto-unarchive the task if it was archived (user is actively editing it)
    let task = if Task::unarchive_if_archived(&deployment.db().pool, task.id).await? {
        // Re-fetch to get updated archived_at = NULL
        Task::find_by_id(&deployment.db().pool, task.id)
            .await?
            .ok_or(ApiError::Database(SqlxError::RowNotFound))?
    } else {
        task
    };

    if let Some(image_ids) = &payload.image_ids {
        TaskImage::delete_by_task_id(&deployment.db().pool, task.id).await?;
        TaskImage::associate_many_dedup(&deployment.db().pool, task.id, image_ids).await?;
    }

    // If task has been shared, broadcast update (fire-and-forget to avoid blocking)
    if task.shared_task_id.is_some()
        && let Ok(publisher) = deployment.share_publisher()
    {
        let task_clone = task.clone();
        tokio::spawn(async move {
            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                publisher.update_shared_task(&task_clone),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(e)) => tracing::warn!(?e, "failed to sync shared task update"),
                Err(_) => tracing::warn!("shared task sync timed out"),
            }
        });
    }

    Ok(ResponseJson(ApiResponse::success(task)))
}

// ============================================================================
// Delete
// ============================================================================

pub async fn delete_task(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
) -> Result<(StatusCode, ResponseJson<ApiResponse<()>>), ApiError> {
    // Check if this is a task synced from Hive (has shared_task_id)
    if task.shared_task_id.is_some() {
        return delete_remote_task(&deployment, &task).await;
    }

    // Local task: existing logic
    // Validate no running execution processes
    if deployment
        .container()
        .has_running_processes(task.id)
        .await?
    {
        return Err(ApiError::Conflict("Task has running execution processes. Please wait for them to complete or stop them first.".to_string()));
    }

    // Gather task attempts data needed for background cleanup
    let attempts = TaskAttempt::fetch_all(&deployment.db().pool, Some(task.id))
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch task attempts for task {}: {}", task.id, e);
            ApiError::TaskAttempt(e)
        })?;

    // Gather cleanup data before deletion
    let project = task
        .parent_project(&deployment.db().pool)
        .await?
        .ok_or_else(|| ApiError::Database(SqlxError::RowNotFound))?;

    let cleanup_args: Vec<WorktreeCleanup> = attempts
        .iter()
        .filter_map(|attempt| {
            attempt
                .container_ref
                .as_ref()
                .map(|worktree_path| WorktreeCleanup {
                    worktree_path: PathBuf::from(worktree_path),
                    git_repo_path: Some(project.git_repo_path.clone()),
                })
        })
        .collect();

    // Fire-and-forget remote deletion to avoid blocking local operation
    if let Some(shared_task_id) = task.shared_task_id
        && let Ok(publisher) = deployment.share_publisher()
    {
        tokio::spawn(async move {
            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                publisher.delete_shared_task(shared_task_id),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(e)) => tracing::warn!(?e, "failed to sync shared task deletion"),
                Err(_) => tracing::warn!("shared task deletion sync timed out"),
            }
        });
    }

    // Use a transaction to ensure atomicity: either all operations succeed or all are rolled back
    let mut tx = deployment.db().pool.begin().await?;

    // Nullify parent_task_id for all child tasks before deletion
    // This breaks parent-child relationships to avoid foreign key constraint violations
    let total_children_affected = Task::nullify_children_by_parent_id(&mut *tx, task.id).await?;

    // Delete task from database (FK CASCADE will handle task_attempts)
    let rows_affected = Task::delete(&mut *tx, task.id).await?;

    if rows_affected == 0 {
        return Err(ApiError::Database(SqlxError::RowNotFound));
    }

    // Commit the transaction - if this fails, all changes are rolled back
    tx.commit().await?;

    if total_children_affected > 0 {
        tracing::info!(
            "Nullified {} child task references before deleting task {}",
            total_children_affected,
            task.id
        );
    }

    // Spawn background worktree cleanup task
    let task_id = task.id;
    tokio::spawn(async move {
        let span = tracing::info_span!("background_worktree_cleanup", task_id = %task_id);
        let _enter = span.enter();

        tracing::info!(
            "Starting background cleanup for task {} ({} worktrees)",
            task_id,
            cleanup_args.len()
        );

        if let Err(e) = WorktreeManager::batch_cleanup_worktrees(&cleanup_args).await {
            tracing::error!(
                "Background worktree cleanup failed for task {}: {}",
                task_id,
                e
            );
        } else {
            tracing::info!("Background cleanup completed for task {}", task_id);
        }
    });

    // Return 202 Accepted to indicate deletion was scheduled
    Ok((StatusCode::ACCEPTED, ResponseJson(ApiResponse::success(()))))
}
