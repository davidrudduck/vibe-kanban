//! Label publisher for syncing labels to the Hive.
//!
//! This service handles syncing local labels to the Hive server when connected.
//! It publishes label create/update/delete operations to keep the Hive in sync
//! with local label state.

use db::{DBService, models::label::Label};
use tracing::{debug, info, warn};
use uuid::Uuid;

use super::ShareError;
use crate::services::remote_client::{
    CreateLabelRequest, DeleteLabelRequest, HiveLabel, RemoteClient, UpdateLabelRequest,
};

/// Publisher for syncing labels to the Hive.
#[derive(Clone)]
pub struct LabelPublisher {
    db: DBService,
    client: RemoteClient,
}

impl LabelPublisher {
    pub fn new(db: DBService, client: RemoteClient) -> Self {
        Self { db, client }
    }

    /// Sync a local label to the Hive.
    ///
    /// If the label has no `swarm_label_id`, it will be created in the Hive.
    /// If it already has one, it will be updated.
    pub async fn sync_label(
        &self,
        label_id: Uuid,
        organization_id: Uuid,
        node_id: Option<Uuid>,
    ) -> Result<Uuid, ShareError> {
        let label = Label::find_by_id(&self.db.pool, label_id)
            .await?
            .ok_or(ShareError::LabelNotFound(label_id))?;

        if let Some(swarm_label_id) = label.swarm_label_id {
            // Label already synced, update it
            self.update_label_in_hive(&label, swarm_label_id).await?;
            Ok(swarm_label_id)
        } else {
            // Label not yet synced, create it in Hive
            let hive_label = self
                .create_label_in_hive(&label, organization_id, node_id)
                .await?;
            // Update local label with swarm_label_id
            Label::set_swarm_label_id(&self.db.pool, label_id, hive_label.id).await?;
            info!(
                local_label_id = %label_id,
                swarm_label_id = %hive_label.id,
                "Label synced to Hive"
            );
            Ok(hive_label.id)
        }
    }

    /// Create a label in the Hive.
    async fn create_label_in_hive(
        &self,
        label: &Label,
        organization_id: Uuid,
        node_id: Option<Uuid>,
    ) -> Result<HiveLabel, ShareError> {
        let request = CreateLabelRequest {
            organization_id,
            project_id: label.project_id,
            origin_node_id: node_id,
            name: label.name.clone(),
            icon: label.icon.clone(),
            color: label.color.clone(),
        };

        let response = self.client.create_label(&request).await?;
        debug!(
            label_name = %label.name,
            hive_label_id = %response.label.id,
            "Created label in Hive"
        );
        Ok(response.label)
    }

    /// Update a label in the Hive.
    async fn update_label_in_hive(
        &self,
        label: &Label,
        swarm_label_id: Uuid,
    ) -> Result<HiveLabel, ShareError> {
        let request = UpdateLabelRequest {
            name: Some(label.name.clone()),
            icon: Some(label.icon.clone()),
            color: Some(label.color.clone()),
            version: Some(label.version),
        };

        let response = self.client.update_label(swarm_label_id, &request).await?;
        Label::mark_synced(&self.db.pool, label.id).await?;
        debug!(
            label_name = %label.name,
            swarm_label_id = %swarm_label_id,
            "Updated label in Hive"
        );
        Ok(response.label)
    }

    /// Delete a label from the Hive.
    pub async fn delete_label(&self, label_id: Uuid) -> Result<(), ShareError> {
        let label = Label::find_by_id(&self.db.pool, label_id)
            .await?
            .ok_or(ShareError::LabelNotFound(label_id))?;

        let Some(swarm_label_id) = label.swarm_label_id else {
            // Label was never synced to Hive, nothing to do
            debug!(
                label_id = %label_id,
                "Label not synced to Hive, skipping delete"
            );
            return Ok(());
        };

        let request = DeleteLabelRequest {
            version: Some(label.version),
        };

        match self
            .client
            .delete_label(swarm_label_id, Some(&request))
            .await
        {
            Ok(_) => {
                info!(
                    label_id = %label_id,
                    swarm_label_id = %swarm_label_id,
                    "Deleted label from Hive"
                );
                Ok(())
            }
            Err(e) => {
                warn!(
                    label_id = %label_id,
                    swarm_label_id = %swarm_label_id,
                    error = ?e,
                    "Failed to delete label from Hive"
                );
                Err(e.into())
            }
        }
    }

    /// Sync all unsynced labels to the Hive.
    ///
    /// Returns the number of labels synced.
    pub async fn sync_unsynced_labels(
        &self,
        organization_id: Uuid,
        node_id: Option<Uuid>,
    ) -> Result<usize, ShareError> {
        let unsynced = Label::find_unsynced(&self.db.pool).await?;

        if unsynced.is_empty() {
            debug!("No unsynced labels to sync");
            return Ok(0);
        }

        let mut synced_count = 0;
        for label in unsynced {
            match self.sync_label(label.id, organization_id, node_id).await {
                Ok(_) => {
                    synced_count += 1;
                }
                Err(e) => {
                    warn!(
                        label_id = %label.id,
                        label_name = %label.name,
                        error = ?e,
                        "Failed to sync label to Hive"
                    );
                }
            }
        }

        if synced_count > 0 {
            info!(synced_count, "Synced unsynced labels to Hive");
        }

        Ok(synced_count)
    }

    /// Sync labels that have been modified since last sync.
    ///
    /// Returns the number of labels synced.
    pub async fn sync_modified_labels(&self) -> Result<usize, ShareError> {
        let modified = Label::find_modified_since_sync(&self.db.pool).await?;

        if modified.is_empty() {
            debug!("No modified labels to sync");
            return Ok(0);
        }

        let mut synced_count = 0;
        for label in modified {
            if let Some(swarm_label_id) = label.swarm_label_id {
                match self.update_label_in_hive(&label, swarm_label_id).await {
                    Ok(_) => {
                        synced_count += 1;
                    }
                    Err(e) => {
                        warn!(
                            label_id = %label.id,
                            label_name = %label.name,
                            error = ?e,
                            "Failed to sync modified label to Hive"
                        );
                    }
                }
            }
        }

        if synced_count > 0 {
            info!(synced_count, "Synced modified labels to Hive");
        }

        Ok(synced_count)
    }
}
