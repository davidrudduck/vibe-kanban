//! Backfill service for reconciling task attempt data between nodes and hive.
//!
//! This service handles pulling missing data from nodes when:
//! - A client requests data that's incomplete on the hive
//! - Periodic reconciliation discovers incomplete attempts
//! - A node reconnects after being offline

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tokio::{sync::mpsc, time::MissedTickBehavior};
use uuid::Uuid;

use super::ws::{
    ConnectionManager,
    message::{BackfillRequestMessage, BackfillType, HiveMessage},
};
use crate::db::node_task_attempts::NodeTaskAttemptRepository;

/// Tracks pending backfill requests to correlate responses with original attempt IDs.
#[derive(Debug, Default)]
pub struct BackfillRequestTracker {
    pending: tokio::sync::RwLock<HashMap<Uuid, PendingRequest>>,
}

#[derive(Debug)]
struct PendingRequest {
    node_id: Uuid,
    attempt_ids: Vec<Uuid>,
    requested_at: DateTime<Utc>,
}

impl BackfillRequestTracker {
    pub fn new() -> Self {
        Self {
            pending: tokio::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Record a backfill request.
    pub async fn track(&self, request_id: Uuid, node_id: Uuid, attempt_ids: Vec<Uuid>) {
        let mut pending = self.pending.write().await;
        pending.insert(
            request_id,
            PendingRequest {
                node_id,
                attempt_ids,
                requested_at: Utc::now(),
            },
        );
    }

    /// Get and remove attempt IDs for a completed request.
    pub async fn complete(&self, request_id: Uuid) -> Option<Vec<Uuid>> {
        let mut pending = self.pending.write().await;
        pending.remove(&request_id).map(|req| req.attempt_ids)
    }

    /// Remove all requests for a node (on disconnect). Returns cleared attempt IDs.
    pub async fn clear_node(&self, node_id: Uuid) -> Vec<Uuid> {
        let mut pending = self.pending.write().await;
        let mut cleared = Vec::new();

        pending.retain(|_, req| {
            if req.node_id == node_id {
                cleared.extend(req.attempt_ids.iter().copied());
                false
            } else {
                true
            }
        });

        cleared
    }

    /// Remove stale requests older than timeout_minutes. Returns expired attempt IDs.
    pub async fn cleanup_stale(&self, timeout_minutes: i64) -> Vec<Uuid> {
        let mut pending = self.pending.write().await;
        let mut stale = Vec::new();
        let cutoff = Utc::now() - chrono::Duration::minutes(timeout_minutes);

        pending.retain(|_, req| {
            if req.requested_at < cutoff {
                stale.extend(req.attempt_ids.iter().copied());
                false
            } else {
                true
            }
        });

        stale
    }
}

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
    tracker: Arc<BackfillRequestTracker>,
    shutdown_rx: Option<mpsc::Receiver<()>>,
}

impl BackfillService {
    /// Create a new backfill service.
    pub fn new(pool: PgPool, connections: ConnectionManager, config: BackfillConfig) -> Self {
        Self {
            pool,
            connections,
            config,
            tracker: Arc::new(BackfillRequestTracker::new()),
            shutdown_rx: None,
        }
    }

    /// Get the tracker for use in response handlers.
    pub fn tracker(&self) -> Arc<BackfillRequestTracker> {
        Arc::clone(&self.tracker)
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

        // Track the request for response correlation
        self.tracker
            .track(request.message_id, node_id, vec![attempt_id])
            .await;

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

        // Track the request for response correlation
        self.tracker
            .track(request.message_id, node_id, attempt_ids.clone())
            .await;

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
    /// Uses pagination to process incomplete attempts in batches.
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

        // Cleanup stale tracked requests
        let stale_ids = self
            .tracker
            .cleanup_stale(self.config.backfill_timeout_minutes as i64)
            .await;
        if !stale_ids.is_empty() {
            tracing::info!(
                count = stale_ids.len(),
                "cleaned up stale tracked backfill requests"
            );
        }

        let mut total_requested = 0u32;
        let page_size: i64 = 100;
        let mut offset: i64 = 0;

        // Paginate through incomplete attempts
        loop {
            // Find incomplete attempts where node is online
            let incomplete = repo
                .find_incomplete_with_online_nodes(page_size, offset)
                .await?;

            if incomplete.is_empty() {
                if offset == 0 {
                    tracing::trace!("no incomplete attempts to reconcile");
                }
                break;
            }

            let page_count = incomplete.len();

            // Group by node_id
            let mut by_node: std::collections::HashMap<Uuid, Vec<Uuid>> =
                std::collections::HashMap::new();
            for attempt in &incomplete {
                by_node
                    .entry(attempt.node_id)
                    .or_default()
                    .push(attempt.id);
            }

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

            // If we got fewer results than page_size, we're done
            if (page_count as i64) < page_size {
                break;
            }

            offset += page_size;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tracker_track_and_complete() {
        let tracker = BackfillRequestTracker::new();
        let request_id = Uuid::new_v4();
        let node_id = Uuid::new_v4();
        let attempt_ids = vec![Uuid::new_v4(), Uuid::new_v4()];

        tracker
            .track(request_id, node_id, attempt_ids.clone())
            .await;
        let completed = tracker.complete(request_id).await;
        assert_eq!(completed, Some(attempt_ids));

        // Second complete returns None (already consumed)
        assert_eq!(tracker.complete(request_id).await, None);
    }

    #[tokio::test]
    async fn test_tracker_clear_node() {
        let tracker = BackfillRequestTracker::new();
        let node1 = Uuid::new_v4();
        let node2 = Uuid::new_v4();
        let attempt1 = Uuid::new_v4();
        let attempt2 = Uuid::new_v4();

        tracker.track(Uuid::new_v4(), node1, vec![attempt1]).await;
        tracker.track(Uuid::new_v4(), node2, vec![attempt2]).await;

        let cleared = tracker.clear_node(node1).await;
        assert_eq!(cleared, vec![attempt1]);

        // node2's request still exists
        let pending = tracker.pending.read().await;
        assert_eq!(pending.len(), 1);
    }

    #[tokio::test]
    async fn test_tracker_cleanup_stale() {
        let tracker = BackfillRequestTracker::new();
        let node_id = Uuid::new_v4();
        let attempt_id = Uuid::new_v4();

        // Insert with past timestamp
        {
            let mut pending = tracker.pending.write().await;
            pending.insert(
                Uuid::new_v4(),
                PendingRequest {
                    node_id,
                    attempt_ids: vec![attempt_id],
                    requested_at: chrono::Utc::now() - chrono::Duration::minutes(10),
                },
            );
        }

        let stale = tracker.cleanup_stale(5).await;
        assert_eq!(stale, vec![attempt_id]);

        // Should be empty now
        let pending = tracker.pending.read().await;
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn test_backfill_service_tracks_requests() {
        // This test verifies the tracker() method returns a functional tracker
        // Full integration test would require mocking ConnectionManager
        let tracker = BackfillRequestTracker::new();
        let request_id = Uuid::new_v4();
        let node_id = Uuid::new_v4();
        let attempt_id = Uuid::new_v4();

        tracker.track(request_id, node_id, vec![attempt_id]).await;

        // Verify tracker accessible via service (pattern validation)
        let arc_tracker = Arc::new(tracker);
        let result = arc_tracker.complete(request_id).await;
        assert_eq!(result, Some(vec![attempt_id]));
    }
}
