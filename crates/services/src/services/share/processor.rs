//! Activity event processor for syncing labels from the Hive.
//!
//! # Note
//!
//! Task sync has been migrated to ElectricSQL. This processor now only handles
//! label events from the activity stream. Task events are received but ignored
//! since ElectricSQL provides real-time task synchronization.

use db::{
    DBService,
    models::{label::Label, project::Project, shared_task::SharedActivityCursor},
};
use remote::{activity::ActivityEvent, db::labels::LabelActivityPayload};
use sqlx::{Sqlite, Transaction};
use tracing::{debug, warn};
use uuid::Uuid;

use super::{ShareConfig, ShareError};
use crate::services::{auth::AuthContext, remote_client::RemoteClient};

/// Processor for handling activity events from the Hive.
///
/// This processor handles:
/// - Label events (create, update, delete) - synced to local database
/// - Task events - ignored (ElectricSQL handles task sync)
#[derive(Clone)]
pub struct ActivityProcessor {
    db: DBService,
    config: ShareConfig,
    remote_client: RemoteClient,
    #[allow(dead_code)]
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
            // Label events - sync to local database
            "label.created" | "label.updated" => {
                self.process_label_upsert_event(&mut tx, &event).await?
            }
            "label.deleted" => self.process_label_deleted_event(&mut tx, &event).await?,

            // Task events - ignored (ElectricSQL handles task sync)
            "task.created" | "task.updated" | "task.deleted" => {
                debug!(
                    event_type = %event.event_type,
                    event_id = %event.event_id,
                    "Ignoring task event - ElectricSQL handles task sync"
                );
            }

            // Unknown events
            _ => {
                debug!(
                    event_type = %event.event_type,
                    event_id = %event.event_id,
                    "Ignoring unknown event type"
                );
            }
        }

        // Always update cursor position
        SharedActivityCursor::upsert(tx.as_mut(), event.project_id, event.seq).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Fetch and process activity events until caught up.
    pub async fn catch_up_project(
        &self,
        remote_project_id: Uuid,
        mut last_seq: Option<i64>,
    ) -> Result<Option<i64>, ShareError> {
        loop {
            let events = self.fetch_activity(remote_project_id, last_seq).await?;
            if events.is_empty() {
                break;
            }

            let page_len = events.len();
            for ev in events {
                if ev.project_id != remote_project_id {
                    warn!(
                        expected = %remote_project_id,
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
        remote_project_id: Uuid,
        after: Option<i64>,
    ) -> Result<Vec<ActivityEvent>, ShareError> {
        let resp = self
            .remote_client
            .fetch_activity(remote_project_id, after, self.config.activity_page_limit)
            .await?;
        Ok(resp.data)
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

        // Check if we already have this label locally (by shared_label_id)
        if let Some(existing) = Label::find_by_shared_label_id(tx.as_mut(), hive_label.id).await? {
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
                    shared_label_id = %hive_label.id,
                    "Updated local label from Hive"
                );
            } else {
                debug!(
                    local_label_id = %existing.id,
                    shared_label_id = %hive_label.id,
                    local_version = existing.version,
                    hive_version = hive_label.version,
                    "Skipping label update - local version is newer or equal"
                );
            }
        } else {
            // Create new local label from Hive
            // Map project_id from remote to local if this is a project-scoped label
            let local_project_id = if let Some(remote_project_id) = hive_label.project_id {
                Project::find_by_remote_project_id(tx.as_mut(), remote_project_id)
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
                shared_label_id = %hive_label.id,
                label_name = %hive_label.name,
                "Created local label from Hive"
            );
        }

        Ok(())
    }

    /// Process a label.deleted event from the Hive.
    /// This removes the shared_label_id from the local label (soft unlink).
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

        // Find local label by shared_label_id and unlink it
        if let Some(existing) = Label::find_by_shared_label_id(tx.as_mut(), hive_label.id).await? {
            // Clear the shared_label_id to unlink from Hive
            // We don't delete the local label - just unlink it
            Label::clear_shared_label_id(tx.as_mut(), existing.id).await?;

            debug!(
                local_label_id = %existing.id,
                shared_label_id = %hive_label.id,
                "Unlinked local label from deleted Hive label"
            );
        }

        Ok(())
    }
}
