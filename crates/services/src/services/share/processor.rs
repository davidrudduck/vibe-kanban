use std::collections::HashSet;

use db::{
    DBService,
    models::{
        label::Label,
        project::Project,
        shared_task::{SharedActivityCursor, SharedTask, SharedTaskInput},
        task::Task,
    },
};
use remote::{
    activity::ActivityEvent,
    db::{labels::LabelActivityPayload, tasks::SharedTaskActivityPayload},
    routes::tasks::BulkSharedTasksResponse,
};
use sqlx::{Sqlite, Transaction};
use tracing::{debug, warn};
use uuid::Uuid;

use super::{
    ShareConfig, ShareError, convert_remote_task, status, sync_local_task_for_shared_task,
};
use crate::services::{auth::AuthContext, remote_client::RemoteClient};

struct PreparedBulkTask {
    input: SharedTaskInput,
    creator_user_id: Option<uuid::Uuid>,
    project: Option<Project>,
    // For remote task upserts
    remote_task_id: Uuid,
    remote_task_title: String,
    remote_task_description: Option<String>,
    remote_task_status: remote::db::tasks::TaskStatus,
    remote_task_assignee_user_id: Option<Uuid>,
    remote_task_version: i64,
    remote_task_archived_at: Option<chrono::DateTime<chrono::Utc>>,
    remote_user_first_name: Option<String>,
    remote_user_last_name: Option<String>,
    remote_user_username: Option<String>,
}

/// Processor for handling activity events and synchronizing shared tasks.
#[derive(Clone)]
pub struct ActivityProcessor {
    db: DBService,
    config: ShareConfig,
    remote_client: RemoteClient,
    auth_ctx: AuthContext,
}

impl ActivityProcessor {
    pub fn new(
        db: DBService,
        config: ShareConfig,
        remote_client: RemoteClient,
        auth_ctx: AuthContext,
    ) -> Self {
        Self {
            db,
            config,
            remote_client,
            auth_ctx,
        }
    }

    pub fn remote_client(&self) -> RemoteClient {
        self.remote_client.clone()
    }

    pub async fn process_event(&self, event: ActivityEvent) -> Result<(), ShareError> {
        let mut tx = self.db.pool.begin().await?;
        match event.event_type.as_str() {
            "task.deleted" => self.process_deleted_task_event(&mut tx, &event).await?,
            "label.created" | "label.updated" => {
                self.process_label_upsert_event(&mut tx, &event).await?
            }
            "label.deleted" => self.process_label_deleted_event(&mut tx, &event).await?,
            _ => self.process_upsert_event(&mut tx, &event).await?,
        }

        SharedActivityCursor::upsert(tx.as_mut(), event.project_id, event.seq).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Fetch and process activity events until caught up, falling back to bulk syncs when needed.
    pub async fn catch_up_project(
        &self,
        swarm_project_id: Uuid,
        mut last_seq: Option<i64>,
    ) -> Result<Option<i64>, ShareError> {
        if last_seq.is_none() {
            last_seq = self.bulk_sync(swarm_project_id).await?;
        }

        loop {
            let events = self.fetch_activity(swarm_project_id, last_seq).await?;
            if events.is_empty() {
                break;
            }

            // Perform a bulk sync if we've fallen too far behind
            if let Some(prev_seq) = last_seq
                && let Some(newest) = events.last()
                && newest.seq.saturating_sub(prev_seq) > self.config.bulk_sync_threshold as i64
            {
                last_seq = self.bulk_sync(swarm_project_id).await?;
                continue;
            }

            let page_len = events.len();
            for ev in events {
                if ev.project_id != swarm_project_id {
                    tracing::warn!(
                        expected = %swarm_project_id,
                        received = %ev.project_id,
                        "received activity for unexpected project; ignoring"
                    );
                    continue;
                }
                self.process_event(ev.clone()).await?;
                last_seq = Some(ev.seq);
            }

            if page_len < (self.config.activity_page_limit as usize) {
                break;
            }
        }

        Ok(last_seq)
    }

    /// Fetch a page of activity events from the remote service.
    async fn fetch_activity(
        &self,
        swarm_project_id: Uuid,
        after: Option<i64>,
    ) -> Result<Vec<ActivityEvent>, ShareError> {
        let resp = self
            .remote_client
            .fetch_activity(swarm_project_id, after, self.config.activity_page_limit)
            .await?;
        Ok(resp.data)
    }

    async fn resolve_project(
        &self,
        task_id: Uuid,
        swarm_project_id: Uuid,
    ) -> Result<Option<Project>, ShareError> {
        if let Some(existing) = SharedTask::find_by_id(&self.db.pool, task_id).await?
            && let Some(project) =
                Project::find_by_swarm_project_id(&self.db.pool, existing.swarm_project_id)
                    .await?
        {
            return Ok(Some(project));
        }

        if let Some(project) =
            Project::find_by_swarm_project_id(&self.db.pool, swarm_project_id).await?
        {
            return Ok(Some(project));
        }

        Ok(None)
    }

    async fn process_upsert_event(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        event: &ActivityEvent,
    ) -> Result<(), ShareError> {
        let Some(payload) = &event.payload else {
            tracing::warn!(event_id = %event.event_id, "received activity event with empty payload");
            return Ok(());
        };

        match serde_json::from_value::<SharedTaskActivityPayload>(payload.clone()) {
            Ok(SharedTaskActivityPayload { task, user }) => {
                let project = self.resolve_project(task.id, event.project_id).await?;
                if project.is_none() {
                    tracing::debug!(
                        task_id = %task.id,
                        swarm_project_id = %task.project_id,
                        "stored shared task without local project; awaiting link"
                    );
                }

                let input = convert_remote_task(
                    &task,
                    user.as_ref(),
                    Some(event.seq),
                    Some(event.created_at),
                );
                let shared_task = SharedTask::upsert(tx.as_mut(), input).await?;

                let current_profile = self.auth_ctx.cached_profile().await;
                let current_user_id = current_profile.as_ref().map(|p| p.user_id);

                // Use mutually exclusive sync paths based on project type to avoid duplicates
                if let Some(ref project) = project {
                    if project.is_remote {
                        // Remote projects: use upsert_remote_task (creates with is_remote=1)
                        let assignee_name = user
                            .as_ref()
                            .map(|u| match (&u.first_name, &u.last_name) {
                                (Some(f), Some(l)) => format!("{} {}", f, l),
                                (Some(f), None) => f.clone(),
                                (None, Some(l)) => l.clone(),
                                (None, None) => String::new(),
                            })
                            .filter(|s| !s.is_empty());

                        Task::upsert_remote_task(
                            tx.as_mut(),
                            Uuid::new_v4(),
                            project.id,
                            task.id,
                            task.title.clone(),
                            task.description.clone(),
                            status::from_remote(&task.status),
                            task.assignee_user_id,
                            assignee_name,
                            user.as_ref().and_then(|u| u.username.clone()),
                            task.version,
                            Some(event.created_at),
                            task.archived_at,
                        )
                        .await?;
                    } else {
                        // Local projects: use sync_local_task_for_shared_task (creates with is_remote=0)
                        sync_local_task_for_shared_task(
                            tx.as_mut(),
                            &shared_task,
                            current_user_id,
                            task.creator_user_id,
                            Some(project.id),
                        )
                        .await?;
                    }
                }
            }
            Err(error) => {
                tracing::warn!(
                    ?error,
                    event_id = %event.event_id,
                    "unrecognized shared task payload; skipping"
                );
            }
        }

        Ok(())
    }

    async fn process_deleted_task_event(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        event: &ActivityEvent,
    ) -> Result<(), ShareError> {
        let Some(payload) = &event.payload else {
            tracing::warn!(
                event_id = %event.event_id,
                "received delete event without payload; skipping"
            );
            return Ok(());
        };

        let SharedTaskActivityPayload { task, .. } =
            match serde_json::from_value::<SharedTaskActivityPayload>(payload.clone()) {
                Ok(payload) => payload,
                Err(error) => {
                    tracing::warn!(
                        ?error,
                        event_id = %event.event_id,
                        "failed to parse deleted task payload; skipping"
                    );
                    return Ok(());
                }
            };

        if let Some(local_task) = Task::find_by_swarm_task_id(tx.as_mut(), task.id).await? {
            Task::set_swarm_task_id(tx.as_mut(), local_task.id, None).await?;
        }

        SharedTask::remove(tx.as_mut(), task.id).await?;

        // Also delete from unified tasks table if it exists as a remote task
        Task::delete_by_swarm_task_id(tx.as_mut(), task.id).await?;

        Ok(())
    }

    async fn bulk_sync(&self, swarm_project_id: Uuid) -> Result<Option<i64>, ShareError> {
        let bulk_resp = self.fetch_bulk_snapshot(swarm_project_id).await?;
        let latest_seq = bulk_resp.latest_seq;

        let mut keep_ids = HashSet::new();
        let mut replacements = Vec::new();

        for payload in bulk_resp.tasks {
            let project = self
                .resolve_project(payload.task.id, swarm_project_id)
                .await?;

            if project.is_none() {
                tracing::debug!(
                    task_id = %payload.task.id,
                    swarm_project_id = %payload.task.project_id,
                    "storing shared task during bulk sync without local project"
                );
            }

            keep_ids.insert(payload.task.id);
            // For bulk sync, use updated_at as activity_at since we don't have the actual event timestamp
            let input = convert_remote_task(
                &payload.task,
                payload.user.as_ref(),
                latest_seq,
                Some(payload.task.updated_at),
            );
            replacements.push(PreparedBulkTask {
                input,
                creator_user_id: payload.task.creator_user_id,
                project: project.clone(),
                // Store data for remote task upsert
                remote_task_id: payload.task.id,
                remote_task_title: payload.task.title.clone(),
                remote_task_description: payload.task.description.clone(),
                remote_task_status: payload.task.status,
                remote_task_assignee_user_id: payload.task.assignee_user_id,
                remote_task_version: payload.task.version,
                remote_task_archived_at: payload.task.archived_at,
                remote_user_first_name: payload.user.as_ref().and_then(|u| u.first_name.clone()),
                remote_user_last_name: payload.user.as_ref().and_then(|u| u.last_name.clone()),
                remote_user_username: payload.user.as_ref().and_then(|u| u.username.clone()),
            });
        }

        let mut stale: HashSet<Uuid> =
            SharedTask::list_by_swarm_project_id(&self.db.pool, swarm_project_id)
                .await?
                .into_iter()
                .filter_map(|task| {
                    if keep_ids.contains(&task.id) {
                        None
                    } else {
                        Some(task.id)
                    }
                })
                .collect();

        for deleted in bulk_resp.deleted_task_ids {
            if !keep_ids.contains(&deleted) {
                stale.insert(deleted);
            }
        }

        let stale_vec: Vec<Uuid> = stale.into_iter().collect();
        let current_profile = self.auth_ctx.cached_profile().await;
        let current_user_id = current_profile.as_ref().map(|p| p.user_id);

        let mut tx = self.db.pool.begin().await?;
        self.remove_stale_tasks(&mut tx, &stale_vec).await?;

        for PreparedBulkTask {
            input,
            creator_user_id,
            project,
            remote_task_id,
            remote_task_title,
            remote_task_description,
            remote_task_status,
            remote_task_assignee_user_id,
            remote_task_version,
            remote_task_archived_at,
            remote_user_first_name,
            remote_user_last_name,
            remote_user_username,
        } in replacements
        {
            let shared_task = SharedTask::upsert(tx.as_mut(), input).await?;

            // Use mutually exclusive sync paths based on project type to avoid duplicates
            if let Some(ref proj) = project {
                if proj.is_remote {
                    // Remote projects: use upsert_remote_task (creates with is_remote=1)
                    let assignee_name = match (&remote_user_first_name, &remote_user_last_name) {
                        (Some(f), Some(l)) => Some(format!("{} {}", f, l)),
                        (Some(f), None) => Some(f.clone()),
                        (None, Some(l)) => Some(l.clone()),
                        (None, None) => None,
                    };

                    Task::upsert_remote_task(
                        tx.as_mut(),
                        Uuid::new_v4(),
                        proj.id,
                        remote_task_id,
                        remote_task_title,
                        remote_task_description,
                        status::from_remote(&remote_task_status),
                        remote_task_assignee_user_id,
                        assignee_name,
                        remote_user_username,
                        remote_task_version,
                        shared_task.activity_at,
                        remote_task_archived_at,
                    )
                    .await?;
                } else {
                    // Local projects: use sync_local_task_for_shared_task (creates with is_remote=0)
                    sync_local_task_for_shared_task(
                        tx.as_mut(),
                        &shared_task,
                        current_user_id,
                        creator_user_id,
                        Some(proj.id),
                    )
                    .await?;
                }
            }
        }

        if let Some(seq) = latest_seq {
            SharedActivityCursor::upsert(tx.as_mut(), swarm_project_id, seq).await?;
        }

        tx.commit().await?;
        Ok(latest_seq)
    }

    async fn remove_stale_tasks(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ids: &[Uuid],
    ) -> Result<(), ShareError> {
        if ids.is_empty() {
            return Ok(());
        }

        for id in ids {
            if let Some(local_task) = Task::find_by_swarm_task_id(tx.as_mut(), *id).await? {
                Task::set_swarm_task_id(tx.as_mut(), local_task.id, None).await?;
            }

            // Also delete from unified tasks table if it exists as a remote task
            Task::delete_by_swarm_task_id(tx.as_mut(), *id).await?;
        }

        SharedTask::remove_many(tx.as_mut(), ids).await?;
        Ok(())
    }

    async fn fetch_bulk_snapshot(
        &self,
        swarm_project_id: Uuid,
    ) -> Result<BulkSharedTasksResponse, ShareError> {
        Ok(self
            .remote_client
            .fetch_bulk_snapshot(swarm_project_id)
            .await?)
    }

    // =========================================================================
    // Label event processing
    // =========================================================================

    /// Process a label.created or label.updated event from the Hive.
    /// This syncs the label from the Hive to the local database.
    async fn process_label_upsert_event(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        event: &ActivityEvent,
    ) -> Result<(), ShareError> {
        let Some(payload) = &event.payload else {
            warn!(
                event_id = %event.event_id,
                "received label upsert event with empty payload"
            );
            return Ok(());
        };

        let label_payload = match serde_json::from_value::<LabelActivityPayload>(payload.clone()) {
            Ok(p) => p,
            Err(error) => {
                warn!(
                    ?error,
                    event_id = %event.event_id,
                    "failed to parse label activity payload; skipping"
                );
                return Ok(());
            }
        };

        let hive_label = label_payload.label;

        // Check if we already have this label locally (by swarm_label_id)
        if let Some(existing) = Label::find_by_swarm_label_id(tx.as_mut(), hive_label.id).await? {
            // Update existing label if the Hive version is newer
            if hive_label.version > existing.version {
                Label::update_from_hive(
                    tx.as_mut(),
                    existing.id,
                    &hive_label.name,
                    &hive_label.icon,
                    &hive_label.color,
                    hive_label.version,
                )
                .await?;
                debug!(
                    local_label_id = %existing.id,
                    swarm_label_id = %hive_label.id,
                    "Updated local label from Hive"
                );
            } else {
                debug!(
                    local_label_id = %existing.id,
                    swarm_label_id = %hive_label.id,
                    local_version = existing.version,
                    hive_version = hive_label.version,
                    "Skipping label update - local version is newer or equal"
                );
            }
        } else {
            // Create new local label from Hive
            // Map project_id from remote to local if this is a project-scoped label
            let local_project_id = if let Some(swarm_project_id) = hive_label.project_id {
                Project::find_by_swarm_project_id(tx.as_mut(), swarm_project_id)
                    .await?
                    .map(|p| p.id)
            } else {
                None
            };

            Label::create_from_hive(
                tx.as_mut(),
                hive_label.id,
                local_project_id,
                &hive_label.name,
                &hive_label.icon,
                &hive_label.color,
                hive_label.version,
            )
            .await?;
            debug!(
                swarm_label_id = %hive_label.id,
                label_name = %hive_label.name,
                "Created local label from Hive"
            );
        }

        Ok(())
    }

    /// Process a label.deleted event from the Hive.
    /// This removes the swarm_label_id from the local label (soft unlink).
    async fn process_label_deleted_event(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        event: &ActivityEvent,
    ) -> Result<(), ShareError> {
        let Some(payload) = &event.payload else {
            warn!(
                event_id = %event.event_id,
                "received label delete event with empty payload"
            );
            return Ok(());
        };

        let label_payload = match serde_json::from_value::<LabelActivityPayload>(payload.clone()) {
            Ok(p) => p,
            Err(error) => {
                warn!(
                    ?error,
                    event_id = %event.event_id,
                    "failed to parse label delete payload; skipping"
                );
                return Ok(());
            }
        };

        let hive_label = label_payload.label;

        // Find local label by swarm_label_id and unlink it
        if let Some(existing) = Label::find_by_swarm_label_id(tx.as_mut(), hive_label.id).await? {
            // Clear the swarm_label_id to unlink from Hive
            // We don't delete the local label - just unlink it
            Label::clear_swarm_label_id(tx.as_mut(), existing.id).await?;

            debug!(
                local_label_id = %existing.id,
                swarm_label_id = %hive_label.id,
                "Unlinked local label from deleted Hive label"
            );
        }

        Ok(())
    }
}
