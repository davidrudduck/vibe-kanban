//! Backfill service for reconciling task attempt data between nodes and hive.
//!
//! This service handles pulling missing data from nodes when:
//! - A client requests data that's incomplete on the hive
//! - Periodic reconciliation discovers incomplete attempts
//! - A node reconnects after being offline

use std::time::Duration;

use sqlx::PgPool;
use tokio::{sync::mpsc, time::MissedTickBehavior};
use uuid::Uuid;

use super::ws::{
    ConnectionManager,
    message::{BackfillRequestMessage, BackfillType, HiveMessage},
};
use crate::db::node_task_attempts::NodeTaskAttemptRepository;

/// Configuration for the backfill service.
#[derive(Debug, Clone)]
pub struct BackfillConfig {
    /// How often to run periodic reconciliation (default: 60 seconds)
    pub reconciliation_interval: Duration,
    /// Maximum number of attempts to backfill in one batch
    pub batch_size: usize,
    /// Timeout for pending backfill requests before retry
    pub backfill_timeout_minutes: i32,
}

impl Default for BackfillConfig {
    fn default() -> Self {
        Self {
            reconciliation_interval: Duration::from_secs(60),
            batch_size: 10,
            backfill_timeout_minutes: 5,
        }
    }
}

/// Service for managing backfill operations.
pub struct BackfillService {
    pool: PgPool,
    connections: ConnectionManager,
    config: BackfillConfig,
    shutdown_rx: Option<mpsc::Receiver<()>>,
}

impl BackfillService {
    /// Create a new backfill service.
    pub fn new(pool: PgPool, connections: ConnectionManager, config: BackfillConfig) -> Self {
        Self {
            pool,
            connections,
            config,
            shutdown_rx: None,
        }
    }

    /// Create with a shutdown receiver for graceful shutdown.
    pub fn with_shutdown(mut self, shutdown_rx: mpsc::Receiver<()>) -> Self {
        self.shutdown_rx = Some(shutdown_rx);
        self
    }

    /// Request immediate backfill for a specific attempt.
    ///
    /// Called when a client requests an attempt that's incomplete on the hive.
    /// This is non-blocking - it sends the request and returns immediately.
    pub async fn request_immediate_backfill(
        &self,
        node_id: Uuid,
        attempt_id: Uuid,
    ) -> Result<(), BackfillError> {
        // Check if node is connected
        if !self.connections.is_connected(node_id).await {
            return Err(BackfillError::NodeOffline(node_id));
        }

        // Mark as pending backfill
        let repo = NodeTaskAttemptRepository::new(&self.pool);
        repo.mark_pending_backfill(&[attempt_id]).await?;

        // Send backfill request
        let request = BackfillRequestMessage {
            message_id: Uuid::new_v4(),
            backfill_type: BackfillType::FullAttempt,
            entity_ids: vec![attempt_id],
            logs_after: None,
        };

        if let Err(e) = self
            .connections
            .send_to_node(node_id, HiveMessage::BackfillRequest(request))
            .await
        {
            tracing::warn!(
                node_id = %node_id,
                attempt_id = %attempt_id,
                error = %e,
                "failed to send backfill request to node"
            );
            return Err(BackfillError::SendFailed(node_id));
        }

        tracing::info!(
            node_id = %node_id,
            attempt_id = %attempt_id,
            "sent immediate backfill request"
        );

        Ok(())
    }

    /// Request backfill for multiple attempts from a node.
    ///
    /// Called during periodic reconciliation or node reconnect.
    pub async fn request_batch_backfill(
        &self,
        node_id: Uuid,
        attempt_ids: Vec<Uuid>,
    ) -> Result<u32, BackfillError> {
        if attempt_ids.is_empty() {
            return Ok(0);
        }

        // Check if node is connected
        if !self.connections.is_connected(node_id).await {
            return Err(BackfillError::NodeOffline(node_id));
        }

        // Mark as pending backfill
        let repo = NodeTaskAttemptRepository::new(&self.pool);
        let marked = repo.mark_pending_backfill(&attempt_ids).await?;

        if marked == 0 {
            // All attempts might already be pending or complete
            tracing::debug!(
                node_id = %node_id,
                "no attempts marked as pending_backfill (may already be pending or complete)"
            );
            return Ok(0);
        }

        // Send backfill request
        let request = BackfillRequestMessage {
            message_id: Uuid::new_v4(),
            backfill_type: BackfillType::FullAttempt,
            entity_ids: attempt_ids.clone(),
            logs_after: None,
        };

        if let Err(e) = self
            .connections
            .send_to_node(node_id, HiveMessage::BackfillRequest(request))
            .await
        {
            tracing::warn!(
                node_id = %node_id,
                attempt_count = attempt_ids.len(),
                error = %e,
                "failed to send batch backfill request to node"
            );
            return Err(BackfillError::SendFailed(node_id));
        }

        tracing::info!(
            node_id = %node_id,
            attempt_count = attempt_ids.len(),
            "sent batch backfill request"
        );

        Ok(marked as u32)
    }

    /// Trigger backfill for a node that just reconnected.
    ///
    /// Called from the session handler when a node successfully authenticates.
    pub async fn trigger_reconnect_backfill(&self, node_id: Uuid) -> Result<u32, BackfillError> {
        let repo = NodeTaskAttemptRepository::new(&self.pool);
        let incomplete = repo.find_incomplete_for_node(node_id).await?;

        if incomplete.is_empty() {
            tracing::debug!(
                node_id = %node_id,
                "no incomplete attempts to backfill on reconnect"
            );
            return Ok(0);
        }

        let attempt_ids: Vec<Uuid> = incomplete
            .iter()
            .take(self.config.batch_size)
            .map(|a| a.id)
            .collect();

        tracing::info!(
            node_id = %node_id,
            total_incomplete = incomplete.len(),
            requesting = attempt_ids.len(),
            "triggering backfill for reconnected node"
        );

        self.request_batch_backfill(node_id, attempt_ids).await
    }

    /// Run periodic reconciliation.
    ///
    /// This finds incomplete attempts where the node is online and requests backfill.
    async fn run_periodic_reconciliation(&self) -> Result<u32, BackfillError> {
        let repo = NodeTaskAttemptRepository::new(&self.pool);

        // Reset stale pending_backfill states first
        let reset = repo
            .reset_stale_pending_backfill(self.config.backfill_timeout_minutes)
            .await?;
        if reset > 0 {
            tracing::info!(
                count = reset,
                "reset stale pending_backfill states"
            );
        }

        // Find incomplete attempts where node is online
        let incomplete = repo.find_incomplete_with_online_nodes().await?;
        if incomplete.is_empty() {
            tracing::trace!("no incomplete attempts to reconcile");
            return Ok(0);
        }

        // Group by node_id
        let mut by_node: std::collections::HashMap<Uuid, Vec<Uuid>> =
            std::collections::HashMap::new();
        for attempt in &incomplete {
            by_node
                .entry(attempt.node_id)
                .or_default()
                .push(attempt.id);
        }

        let mut total_requested = 0u32;
        for (node_id, attempt_ids) in by_node {
            // Limit batch size per node
            let batch: Vec<Uuid> = attempt_ids
                .into_iter()
                .take(self.config.batch_size)
                .collect();

            match self.request_batch_backfill(node_id, batch).await {
                Ok(count) => total_requested += count,
                Err(e) => {
                    tracing::warn!(
                        node_id = %node_id,
                        error = %e,
                        "failed to request batch backfill"
                    );
                }
            }
        }

        if total_requested > 0 {
            tracing::info!(
                count = total_requested,
                "periodic reconciliation requested backfill"
            );
        }

        Ok(total_requested)
    }

    /// Spawn the periodic reconciliation task.
    ///
    /// Returns a join handle that can be used to wait for the task to complete.
    pub fn spawn(mut self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(self.config.reconciliation_interval);
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

            let mut shutdown_rx = self.shutdown_rx.take();

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = self.run_periodic_reconciliation().await {
                            tracing::error!(
                                error = %e,
                                "periodic reconciliation failed"
                            );
                        }
                    }
                    _ = async {
                        match shutdown_rx.as_mut() {
                            Some(rx) => rx.recv().await,
                            None => std::future::pending().await,
                        }
                    } => {
                        tracing::info!("backfill service shutting down");
                        break;
                    }
                }
            }
        })
    }
}

/// Errors from backfill operations.
#[derive(Debug, thiserror::Error)]
pub enum BackfillError {
    #[error("node {0} is offline")]
    NodeOffline(Uuid),
    #[error("failed to send to node {0}")]
    SendFailed(Uuid),
    #[error("database error: {0}")]
    Database(#[from] crate::db::node_task_attempts::NodeTaskAttemptError),
}
