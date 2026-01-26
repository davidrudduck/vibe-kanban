//! Node cache service for syncing node/project data from the hive (legacy implementation).
//!
//! This service fetches all nodes and their projects from the hive
//! and caches them locally in SQLite. This allows the frontend to show a unified
//! view of all projects across all nodes in the organization.
//!
//! The sync can be triggered:
//! - On user login (to sync their organizations)
//! - Periodically as a background task
//! - On-demand when the user views the unified projects page
//!
//! # DEPRECATION NOTICE
//!
//! This module is a candidate for deprecation in a future release. It will be
//! replaced by ElectricSQL-based real-time sync for node/project data.
//!
//! ## Future Migration Path
//!
//! When Electric sync is extended to include node data:
//! - Electric shapes for `nodes` and `node_projects` tables
//! - Real-time updates via PostgreSQL logical replication
//! - No periodic polling required
//!
//! ## Current Status
//!
//! This implementation is still active and required for node discovery.
//! The Electric proxy route (`/api/electric/v1/shape`) is already set up
//! and can be extended to include node shapes.
//!
//! ## See Also
//!
//! - `crates/remote/migrations/20251225000000_electric_support.sql` - nodes table in publication
//! - `crates/remote/src/routes/electric_proxy.rs` - Electric proxy with auth
//! - `frontend/src/lib/electric/collections.ts` - TanStack DB collections (includes nodes)

use std::sync::Arc;
use std::time::Duration;

use db::models::cached_node::{
    CachedNode, CachedNodeCapabilities, CachedNodeInput, CachedNodeStatus,
};
use remote::nodes::Node;
use sqlx::SqlitePool;
use tokio::sync::RwLock;
use tokio::time::{self, MissedTickBehavior};
use tracing::{debug, info, warn};
use uuid::Uuid;

use super::remote_client::{RemoteClient, RemoteClientError};

/// Default sync interval (5 minutes)
const DEFAULT_SYNC_INTERVAL: Duration = Duration::from_secs(300);

/// Sync nodes and projects for an organization.
///
/// This is a stateless function that can be called from anywhere.
/// It fetches nodes and projects from the remote API and caches them locally.
///
/// If `current_node_id` is provided, projects from that node will NOT be synced
/// as remote entries (since they are local projects, not remote ones).
pub async fn sync_organization(
    pool: &SqlitePool,
    remote_client: &RemoteClient,
    organization_id: Uuid,
    current_node_id: Option<Uuid>,
) -> Result<SyncStats, NodeCacheSyncError> {
    let syncer = NodeCacheSyncer::new(pool, remote_client, organization_id, current_node_id);
    syncer.sync().await
}

/// Sync all organizations the user has access to.
///
/// Fetches the list of organizations from the remote API and syncs nodes
/// for each one.
pub async fn sync_all_organizations(
    pool: &SqlitePool,
    remote_client: &RemoteClient,
) -> Result<Vec<(Uuid, SyncStats)>, NodeCacheSyncError> {
    debug!("fetching organizations for node cache sync");

    let orgs = match remote_client.list_organizations().await {
        Ok(orgs) => orgs,
        Err(e) => {
            warn!(error = %e, "failed to fetch organizations for node cache sync");
            return Err(NodeCacheSyncError::Remote(e));
        }
    };

    if orgs.organizations.is_empty() {
        info!("no organizations found, skipping node cache sync");
        return Ok(vec![]);
    }

    info!(
        org_count = orgs.organizations.len(),
        "fetched organizations for node cache sync"
    );

    let mut results = Vec::with_capacity(orgs.organizations.len());

    for org in orgs.organizations {
        match sync_organization(pool, remote_client, org.id, None).await {
            Ok(stats) => {
                info!(
                    organization_id = %org.id,
                    organization_name = %org.name,
                    nodes_synced = stats.nodes_synced,
                    "synced organization nodes"
                );
                results.push((org.id, stats));
            }
            Err(e) => {
                warn!(
                    organization_id = %org.id,
                    error = %e,
                    "failed to sync organization nodes"
                );
            }
        }
    }

    Ok(results)
}

/// Internal syncer for a single organization.
struct NodeCacheSyncer<'a> {
    pool: &'a SqlitePool,
    remote_client: &'a RemoteClient,
    organization_id: Uuid,
    /// If set, skip syncing projects from this node (they're local, not remote)
    current_node_id: Option<Uuid>,
}

impl<'a> NodeCacheSyncer<'a> {
    fn new(
        pool: &'a SqlitePool,
        remote_client: &'a RemoteClient,
        organization_id: Uuid,
        current_node_id: Option<Uuid>,
    ) -> Self {
        Self {
            pool,
            remote_client,
            organization_id,
            current_node_id,
        }
    }

    /// Perform a single sync operation
    async fn sync(&self) -> Result<SyncStats, NodeCacheSyncError> {
        let org_id = self.organization_id;
        let mut stats = SyncStats::default();

        debug!(organization_id = %org_id, "fetching nodes for organization");

        // Fetch all nodes from the hive
        let nodes = match self.remote_client.list_nodes(org_id).await {
            Ok(nodes) => nodes,
            Err(e) => {
                warn!(organization_id = %org_id, error = %e, "failed to fetch nodes from hive");
                return Err(NodeCacheSyncError::Remote(e));
            }
        };

        info!(
            organization_id = %org_id,
            node_count = nodes.len(),
            "fetched nodes from hive"
        );

        let mut synced_node_ids = Vec::with_capacity(nodes.len());

        // Upsert each node
        for node in nodes {
            let node_id = node.id;
            synced_node_ids.push(node_id);

            // Convert and upsert the node
            let input = self.node_to_input(&node);
            match CachedNode::upsert(self.pool, input).await {
                Ok(cached) => {
                    debug!(
                        cached_node_id = %cached.id,
                        cached_node_name = %cached.name,
                        "successfully cached node"
                    );
                    stats.nodes_synced += 1;
                }
                Err(e) => {
                    tracing::error!(
                        node_id = %node_id,
                        error = %e,
                        "failed to upsert cached node"
                    );
                    return Err(NodeCacheSyncError::Database(e));
                }
            }

            // Fetch and sync projects for this node
            // Skip syncing projects from our own node - those are local, not remote
            if Some(node_id) == self.current_node_id {
                debug!(node_id = %node_id, "skipping project sync for current node (local projects)");
            } else {
                match self.sync_node_projects(&node).await {
                    Ok(project_stats) => {
                        stats.projects_synced += project_stats.0;
                        stats.projects_removed += project_stats.1;
                    }
                    Err(e) => {
                        warn!(node_id = %node_id, error = %e, "failed to sync projects for node");
                    }
                }
            }
        }

        // Remove stale nodes (nodes no longer in the hive)
        let removed = CachedNode::remove_stale(self.pool, org_id, &synced_node_ids)
            .await
            .map_err(NodeCacheSyncError::Database)?;
        stats.nodes_removed = removed as usize;

        Ok(stats)
    }

    /// DEPRECATED: Remote project sync is disabled.
    ///
    /// We now fetch swarm projects directly from the Hive instead of caching
    /// remote project entries locally. This eliminates UNIQUE constraint violations
    /// and stale data issues.
    ///
    /// # Returns
    ///
    /// Always returns `(0, 0)` - no projects synced or removed.
    #[allow(clippy::unused_async)]
    async fn sync_node_projects(&self, node: &Node) -> Result<(usize, usize), NodeCacheSyncError> {
        debug!(
            node_id = %node.id,
            "remote project sync disabled - using hive directly"
        );
        Ok((0, 0))
    }

    /// Convert a remote Node to a CachedNodeInput
    fn node_to_input(&self, node: &Node) -> CachedNodeInput {
        CachedNodeInput {
            id: node.id,
            organization_id: node.organization_id,
            name: node.name.clone(),
            machine_id: node.machine_id.clone(),
            status: self.convert_status(&node.status),
            capabilities: CachedNodeCapabilities {
                executors: node.capabilities.executors.clone(),
                max_concurrent_tasks: node.capabilities.max_concurrent_tasks,
                os: node.capabilities.os.clone(),
                arch: node.capabilities.arch.clone(),
                version: node.capabilities.version.clone(),
            },
            public_url: node.public_url.clone(),
            last_heartbeat_at: node.last_heartbeat_at,
            connected_at: node.connected_at,
            disconnected_at: node.disconnected_at,
            created_at: node.created_at,
            updated_at: node.updated_at,
        }
    }

    /// Convert remote NodeStatus to CachedNodeStatus
    fn convert_status(&self, status: &remote::nodes::NodeStatus) -> CachedNodeStatus {
        match status {
            remote::nodes::NodeStatus::Pending => CachedNodeStatus::Pending,
            remote::nodes::NodeStatus::Online => CachedNodeStatus::Online,
            remote::nodes::NodeStatus::Offline => CachedNodeStatus::Offline,
            remote::nodes::NodeStatus::Busy => CachedNodeStatus::Busy,
            remote::nodes::NodeStatus::Draining => CachedNodeStatus::Draining,
        }
    }
}

/// Background sync service that periodically syncs all organizations.
pub struct NodeCacheSyncService {
    pool: SqlitePool,
    remote_client: RemoteClient,
    sync_interval: Duration,
    /// Stop signal
    stop: Arc<RwLock<bool>>,
}

impl NodeCacheSyncService {
    /// Create a new background sync service
    pub fn new(pool: SqlitePool, remote_client: RemoteClient) -> Self {
        Self {
            pool,
            remote_client,
            sync_interval: DEFAULT_SYNC_INTERVAL,
            stop: Arc::new(RwLock::new(false)),
        }
    }

    /// Set the sync interval
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.sync_interval = interval;
        self
    }

    /// Run the background sync loop
    pub async fn run(self) {
        let mut interval = time::interval(self.sync_interval);
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        // Sync immediately on startup
        self.do_sync().await;

        loop {
            interval.tick().await;

            if *self.stop.read().await {
                info!("node cache sync service stopped");
                break;
            }

            self.do_sync().await;
        }
    }

    async fn do_sync(&self) {
        match sync_all_organizations(&self.pool, &self.remote_client).await {
            Ok(results) => {
                let total_nodes: usize = results.iter().map(|(_, s)| s.nodes_synced).sum();
                let total_projects: usize = results.iter().map(|(_, s)| s.projects_synced).sum();
                info!(
                    organizations = results.len(),
                    nodes = total_nodes,
                    projects = total_projects,
                    "node cache sync completed"
                );
            }
            Err(e) => {
                warn!(error = %e, "node cache sync failed");
            }
        }
    }

    /// Request the service to stop
    pub async fn stop(&self) {
        *self.stop.write().await = true;
    }
}

/// Statistics from a sync operation
#[derive(Debug, Default, Clone)]
pub struct SyncStats {
    pub nodes_synced: usize,
    pub nodes_removed: usize,
    pub projects_synced: usize,
    pub projects_removed: usize,
}

/// Errors from the node cache sync service
#[derive(Debug, thiserror::Error)]
pub enum NodeCacheSyncError {
    #[error("remote client error: {0}")]
    Remote(#[from] RemoteClientError),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}
