use chrono::{Duration, Utc};
use rand::Rng;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use thiserror::Error;
use tracing::{info, warn};
use uuid::Uuid;

use super::domain::{
    CreateNodeApiKey, HeartbeatPayload, LinkProjectData, Node, NodeApiKey, NodeProject,
    NodeRegistration, NodeStatus, NodeTaskAssignment, UpdateAssignmentData,
};

/// Type alias for registration data (used by WebSocket session)
pub type RegisterNode = NodeRegistration;
use crate::db::{
    node_api_keys::{NodeApiKeyError, NodeApiKeyRepository},
    node_projects::{NodeProjectError, NodeProjectRepository},
    nodes::{NodeDbError, NodeRepository},
    task_assignments::{TaskAssignmentError, TaskAssignmentRepository},
};

/// Result of a node merge operation.
#[derive(Debug, Clone)]
pub struct MergeNodesResult {
    /// ID of the node that was merged (and deleted)
    pub source_node_id: Uuid,
    /// ID of the node that received the merged data
    pub target_node_id: Uuid,
    /// Number of projects moved from source to target
    pub projects_moved: u64,
    /// Number of API keys rebound from source to target
    pub keys_rebound: u64,
}

/// API key prefix used for identification (first 8 characters of the key)
const API_KEY_PREFIX_LEN: usize = 8;

/// Full API key length (32 bytes = 64 hex chars)
const API_KEY_LEN: usize = 32;

/// Node is considered offline if no heartbeat received within this threshold (seconds)
const NODE_OFFLINE_THRESHOLD_SECS: i64 = 90;

/// Takeover detection window duration (minutes)
const TAKEOVER_WINDOW_MINUTES: i64 = 5;

/// Maximum takeover attempts allowed before blocking the API key
const MAX_TAKEOVER_ATTEMPTS: i32 = 3;

#[derive(Debug, Error)]
pub enum NodeError {
    #[error("node not found")]
    NodeNotFound,
    #[error("API key not found")]
    ApiKeyNotFound,
    #[error("API key invalid")]
    ApiKeyInvalid,
    #[error("API key revoked")]
    ApiKeyRevoked,
    #[error("API key blocked: {0}")]
    ApiKeyBlocked(String),
    #[error("API key already bound to a different node")]
    ApiKeyAlreadyBound,
    #[error("project already linked to a node")]
    ProjectAlreadyLinked,
    #[error("task already has an active assignment")]
    TaskAlreadyAssigned,
    #[error("assignment not found")]
    AssignmentNotFound,
    #[error("node project link not found")]
    NodeProjectNotFound,
    #[error("takeover detected: {0}")]
    TakeoverDetected(String),
    #[error("database error: {0}")]
    Database(String),
}

impl From<NodeDbError> for NodeError {
    fn from(err: NodeDbError) -> Self {
        match err {
            NodeDbError::NotFound => NodeError::NodeNotFound,
            NodeDbError::AlreadyExists => NodeError::Database("node already exists".to_string()),
            NodeDbError::Database(e) => NodeError::Database(e.to_string()),
        }
    }
}

impl From<NodeApiKeyError> for NodeError {
    fn from(err: NodeApiKeyError) -> Self {
        match err {
            NodeApiKeyError::NotFound => NodeError::ApiKeyNotFound,
            NodeApiKeyError::Revoked => NodeError::ApiKeyRevoked,
            NodeApiKeyError::Blocked(reason) => NodeError::ApiKeyBlocked(reason),
            NodeApiKeyError::AlreadyBound => NodeError::ApiKeyAlreadyBound,
            NodeApiKeyError::Database(e) => NodeError::Database(e.to_string()),
        }
    }
}

impl From<NodeProjectError> for NodeError {
    fn from(err: NodeProjectError) -> Self {
        match err {
            NodeProjectError::NotFound => NodeError::NodeProjectNotFound,
            NodeProjectError::ProjectAlreadyLinked => NodeError::ProjectAlreadyLinked,
            NodeProjectError::LocalProjectAlreadyLinked => {
                NodeError::Database("local project already linked".to_string())
            }
            NodeProjectError::Database(e) => NodeError::Database(e.to_string()),
        }
    }
}

impl From<TaskAssignmentError> for NodeError {
    fn from(err: TaskAssignmentError) -> Self {
        match err {
            TaskAssignmentError::NotFound => NodeError::AssignmentNotFound,
            TaskAssignmentError::AlreadyAssigned => NodeError::TaskAlreadyAssigned,
            TaskAssignmentError::Database(e) => NodeError::Database(e.to_string()),
        }
    }
}

/// Service for managing nodes in the swarm
pub struct NodeServiceImpl {
    pool: PgPool,
}

impl NodeServiceImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    // =========================================================================
    // API Key Management
    // =========================================================================

    /// Create a new API key for an organization.
    /// Returns the key record and the raw key value (only available at creation).
    pub async fn create_api_key(
        &self,
        organization_id: Uuid,
        data: CreateNodeApiKey,
        created_by: Uuid,
    ) -> Result<(NodeApiKey, String), NodeError> {
        // Generate a random API key
        let raw_key = generate_api_key();
        let key_prefix = &raw_key[..API_KEY_PREFIX_LEN];
        let key_hash = hash_api_key(&raw_key);

        let repo = NodeApiKeyRepository::new(&self.pool);
        let key = repo
            .create(organization_id, data, created_by, &key_hash, key_prefix)
            .await?;

        Ok((key, raw_key))
    }

    /// Validate an API key and return the key record if valid
    pub async fn validate_api_key(&self, raw_key: &str) -> Result<NodeApiKey, NodeError> {
        if raw_key.len() < API_KEY_PREFIX_LEN {
            return Err(NodeError::ApiKeyInvalid);
        }

        let prefix = &raw_key[..API_KEY_PREFIX_LEN];
        let repo = NodeApiKeyRepository::new(&self.pool);

        let key = repo
            .find_by_prefix(prefix)
            .await?
            .ok_or(NodeError::ApiKeyNotFound)?;

        // Check if key is revoked
        if key.revoked_at.is_some() {
            return Err(NodeError::ApiKeyRevoked);
        }

        // Verify the full hash
        let expected_hash = hash_api_key(raw_key);
        if key.key_hash != expected_hash {
            return Err(NodeError::ApiKeyInvalid);
        }

        // Update last used timestamp
        repo.touch(key.id).await?;

        Ok(key)
    }

    /// List all API keys for an organization
    pub async fn list_api_keys(&self, organization_id: Uuid) -> Result<Vec<NodeApiKey>, NodeError> {
        let repo = NodeApiKeyRepository::new(&self.pool);
        Ok(repo.list_by_organization(organization_id).await?)
    }

    /// Revoke an API key
    pub async fn revoke_api_key(&self, key_id: Uuid) -> Result<(), NodeError> {
        let repo = NodeApiKeyRepository::new(&self.pool);
        Ok(repo.revoke(key_id).await?)
    }

    /// Delete an API key
    pub async fn delete_api_key(&self, key_id: Uuid) -> Result<(), NodeError> {
        let repo = NodeApiKeyRepository::new(&self.pool);
        Ok(repo.delete(key_id).await?)
    }

    // =========================================================================
    // Node Management
    // =========================================================================

    /// Register a new node or update an existing one (legacy method)
    /// Use `register_node_with_api_key` for API key-based identity.
    pub async fn register_node(
        &self,
        organization_id: Uuid,
        data: NodeRegistration,
    ) -> Result<Node, NodeError> {
        let repo = NodeRepository::new(&self.pool);
        Ok(repo.upsert(organization_id, data).await?)
    }

    /// Register or update a node using API key-based identity.
    ///
    /// This implements the "One API Key = One Node" identity model:
    /// - If the API key has no bound node, creates a new node and binds the key
    /// - If the API key is bound to a node, performs takeover detection
    /// - If takeover is suspicious (node was recently active), may block the key
    ///
    /// Returns the registered/updated node.
    pub async fn register_node_with_api_key(
        &self,
        api_key: &NodeApiKey,
        data: NodeRegistration,
    ) -> Result<Node, NodeError> {
        let node_repo = NodeRepository::new(&self.pool);
        let key_repo = NodeApiKeyRepository::new(&self.pool);

        // Check if key is blocked
        if api_key.blocked_at.is_some() {
            let reason = api_key
                .blocked_reason
                .clone()
                .unwrap_or_else(|| "Unknown reason".to_string());
            return Err(NodeError::ApiKeyBlocked(reason));
        }

        match api_key.node_id {
            None => {
                // First connection with this API key - create node and bind key
                info!(
                    key_id = %api_key.id,
                    key_name = %api_key.name,
                    node_name = %data.name,
                    "Creating new node and binding API key"
                );

                let node = node_repo.upsert(api_key.organization_id, data).await?;
                key_repo.bind_to_node(api_key.id, node.id).await?;

                info!(
                    key_id = %api_key.id,
                    node_id = %node.id,
                    "API key bound to new node"
                );

                Ok(node)
            }
            Some(bound_node_id) => {
                // Key is already bound to a node - check for takeover
                let existing_node = node_repo.find_by_id(bound_node_id).await?;

                match existing_node {
                    Some(node) => {
                        // Check if this is a takeover (different machine_id)
                        if node.machine_id == data.machine_id {
                            // Same machine - just update the node info
                            let updated = node_repo.upsert(api_key.organization_id, data).await?;
                            Ok(updated)
                        } else {
                            // Different machine - potential takeover
                            self.handle_takeover(api_key, &node, data).await
                        }
                    }
                    None => {
                        // Bound node no longer exists (deleted) - create new node and rebind
                        warn!(
                            key_id = %api_key.id,
                            old_node_id = %bound_node_id,
                            "Bound node no longer exists, creating new node"
                        );

                        let node = node_repo.upsert(api_key.organization_id, data).await?;
                        key_repo
                            .update_node_binding(api_key.id, node.id, true)
                            .await?;

                        Ok(node)
                    }
                }
            }
        }
    }

    /// Handle a potential takeover when a different machine tries to use a bound API key.
    ///
    /// Logic:
    /// 1. If the bound node is offline (no heartbeat > 90s), allow takeover
    /// 2. If the bound node is online, increment takeover count
    /// 3. If takeover count exceeds threshold, block the API key
    async fn handle_takeover(
        &self,
        api_key: &NodeApiKey,
        existing_node: &Node,
        data: NodeRegistration,
    ) -> Result<Node, NodeError> {
        let key_repo = NodeApiKeyRepository::new(&self.pool);
        let node_repo = NodeRepository::new(&self.pool);
        let now = Utc::now();

        // Check if the existing node is currently online (active within threshold)
        let is_node_online = existing_node.last_heartbeat_at.is_some_and(|last_hb| {
            now - last_hb < Duration::seconds(NODE_OFFLINE_THRESHOLD_SECS)
        });

        if !is_node_online {
            // Node is offline - allow legitimate takeover (e.g., laptop moved)
            info!(
                key_id = %api_key.id,
                old_node_id = %existing_node.id,
                new_machine = %data.machine_id,
                "Allowing takeover - node was offline"
            );

            // Update node with new connection info
            let node = node_repo.upsert(api_key.organization_id, data).await?;

            // Update key binding if node ID changed (upsert may create new node)
            if node.id != existing_node.id {
                key_repo
                    .update_node_binding(api_key.id, node.id, true)
                    .await?;
            } else {
                // Same node, just reset takeover count
                key_repo.reset_takeover_count(api_key.id).await?;
            }

            Ok(node)
        } else {
            // Node is online - suspicious takeover attempt
            warn!(
                key_id = %api_key.id,
                key_name = %api_key.name,
                existing_node_id = %existing_node.id,
                new_machine = %data.machine_id,
                "Suspicious takeover attempt - node is still online"
            );

            // Increment takeover count
            let updated_key = key_repo
                .increment_takeover(api_key.id, TAKEOVER_WINDOW_MINUTES)
                .await?;

            if updated_key.takeover_count > MAX_TAKEOVER_ATTEMPTS {
                // Too many takeover attempts - block the key
                let reason = format!(
                    "Duplicate key use detected: multiple machines attempting to use same key within {} minutes",
                    TAKEOVER_WINDOW_MINUTES
                );

                warn!(
                    key_id = %api_key.id,
                    key_name = %api_key.name,
                    takeover_count = %updated_key.takeover_count,
                    "Blocking API key due to excessive takeover attempts"
                );

                key_repo.block_key(api_key.id, &reason).await?;

                return Err(NodeError::ApiKeyBlocked(reason));
            }

            // Below threshold - log warning but allow this attempt
            // The existing node keeps running; the new connection is rejected
            Err(NodeError::TakeoverDetected(format!(
                "Another machine is using this API key. Attempt {} of {}. \
                 Wait for the other node to go offline or contact admin.",
                updated_key.takeover_count, MAX_TAKEOVER_ATTEMPTS
            )))
        }
    }

    /// Check if an API key is eligible for connection (not blocked, not revoked)
    pub async fn check_api_key_status(&self, api_key: &NodeApiKey) -> Result<(), NodeError> {
        if api_key.revoked_at.is_some() {
            return Err(NodeError::ApiKeyRevoked);
        }

        if let Some(blocked_reason) = &api_key.blocked_reason {
            return Err(NodeError::ApiKeyBlocked(blocked_reason.clone()));
        }

        Ok(())
    }

    /// Unblock an API key (admin operation)
    pub async fn unblock_api_key(&self, key_id: Uuid) -> Result<NodeApiKey, NodeError> {
        let repo = NodeApiKeyRepository::new(&self.pool);
        Ok(repo.unblock_key(key_id).await?)
    }

    /// Get a node by ID
    pub async fn get_node(&self, node_id: Uuid) -> Result<Node, NodeError> {
        let repo = NodeRepository::new(&self.pool);
        repo.find_by_id(node_id)
            .await?
            .ok_or(NodeError::NodeNotFound)
    }

    /// List all nodes for an organization
    pub async fn list_nodes(&self, organization_id: Uuid) -> Result<Vec<Node>, NodeError> {
        let repo = NodeRepository::new(&self.pool);
        Ok(repo.list_by_organization(organization_id).await?)
    }

    /// Process a heartbeat from a node
    pub async fn heartbeat(
        &self,
        node_id: Uuid,
        payload: HeartbeatPayload,
    ) -> Result<(), NodeError> {
        let repo = NodeRepository::new(&self.pool);
        Ok(repo.heartbeat(node_id, payload.status).await?)
    }

    /// Update a node's public URL
    pub async fn update_node_url(
        &self,
        node_id: Uuid,
        public_url: Option<&str>,
    ) -> Result<(), NodeError> {
        let repo = NodeRepository::new(&self.pool);
        Ok(repo.update_public_url(node_id, public_url).await?)
    }

    /// Delete a node
    pub async fn delete_node(&self, node_id: Uuid) -> Result<(), NodeError> {
        let repo = NodeRepository::new(&self.pool);
        Ok(repo.delete(node_id).await?)
    }

    /// Merge one node into another.
    ///
    /// This operation:
    /// 1. Moves all projects from source node to target node
    /// 2. Rebinds any API keys from source to target
    /// 3. Deletes the source node
    ///
    /// Returns a summary of the merge operation.
    pub async fn merge_nodes(
        &self,
        source_node_id: Uuid,
        target_node_id: Uuid,
    ) -> Result<MergeNodesResult, NodeError> {
        let node_repo = NodeRepository::new(&self.pool);
        let project_repo = NodeProjectRepository::new(&self.pool);
        let key_repo = NodeApiKeyRepository::new(&self.pool);

        // Verify both nodes exist
        let source = node_repo
            .find_by_id(source_node_id)
            .await?
            .ok_or(NodeError::NodeNotFound)?;
        let target = node_repo
            .find_by_id(target_node_id)
            .await?
            .ok_or(NodeError::NodeNotFound)?;

        // Verify both nodes are in the same organization
        if source.organization_id != target.organization_id {
            return Err(NodeError::Database(
                "Cannot merge nodes from different organizations".to_string(),
            ));
        }

        info!(
            source_id = %source_node_id,
            source_name = %source.name,
            target_id = %target_node_id,
            target_name = %target.name,
            "Merging nodes"
        );

        // Move all projects from source to target
        let projects_moved = project_repo
            .bulk_update_node_id(source_node_id, target_node_id)
            .await?;

        info!(
            source_id = %source_node_id,
            target_id = %target_node_id,
            projects_moved = projects_moved,
            "Moved projects to target node"
        );

        // Rebind any API keys from source to target
        let source_keys = key_repo.find_by_node_id(source_node_id).await?;
        let keys_rebound = source_keys.len();

        for key in source_keys {
            key_repo
                .update_node_binding(key.id, target_node_id, true)
                .await?;
        }

        info!(
            source_id = %source_node_id,
            target_id = %target_node_id,
            keys_rebound = keys_rebound,
            "Rebound API keys to target node"
        );

        // Delete the source node
        node_repo.delete(source_node_id).await?;

        info!(
            source_id = %source_node_id,
            target_id = %target_node_id,
            "Source node deleted, merge complete"
        );

        Ok(MergeNodesResult {
            source_node_id,
            target_node_id,
            projects_moved,
            keys_rebound: keys_rebound as u64,
        })
    }

    /// Update a node's status (used by WebSocket session)
    pub async fn update_node_status(
        &self,
        node_id: Uuid,
        status: NodeStatus,
    ) -> Result<(), NodeError> {
        let repo = NodeRepository::new(&self.pool);
        Ok(repo.heartbeat(node_id, status).await?)
    }

    // =========================================================================
    // Project Linking
    // =========================================================================

    /// Link a project to a node
    pub async fn link_project(
        &self,
        node_id: Uuid,
        data: LinkProjectData,
    ) -> Result<NodeProject, NodeError> {
        let repo = NodeProjectRepository::new(&self.pool);
        Ok(repo
            .create(
                node_id,
                data.project_id,
                data.local_project_id,
                &data.git_repo_path,
                &data.default_branch,
            )
            .await?)
    }

    /// Get a project link by project ID
    pub async fn get_project_link(
        &self,
        project_id: Uuid,
    ) -> Result<Option<NodeProject>, NodeError> {
        let repo = NodeProjectRepository::new(&self.pool);
        Ok(repo.find_by_project(project_id).await?)
    }

    /// List all project links for a node
    pub async fn list_node_projects(&self, node_id: Uuid) -> Result<Vec<NodeProject>, NodeError> {
        let repo = NodeProjectRepository::new(&self.pool);
        Ok(repo.list_by_node(node_id).await?)
    }

    /// List all projects in an organization with ownership info.
    ///
    /// Returns all projects linked to any node in the organization,
    /// with information about which node owns each project.
    pub async fn list_organization_projects(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<crate::db::node_projects::OrgProjectInfo>, NodeError> {
        let repo = NodeProjectRepository::new(&self.pool);
        Ok(repo.list_by_organization(organization_id).await?)
    }

    /// Update project sync status
    pub async fn update_project_sync(&self, link_id: Uuid, status: &str) -> Result<(), NodeError> {
        let repo = NodeProjectRepository::new(&self.pool);
        Ok(repo.update_sync_status(link_id, status).await?)
    }

    /// Unlink a project from its node (by project ID only)
    pub async fn unlink_project(&self, project_id: Uuid) -> Result<(), NodeError> {
        let repo = NodeProjectRepository::new(&self.pool);
        Ok(repo.delete_by_project(project_id).await?)
    }

    /// Unlink a project from a specific node.
    ///
    /// This method verifies that the node owns the project link before deleting.
    pub async fn unlink_project_for_node(
        &self,
        node_id: Uuid,
        project_id: Uuid,
    ) -> Result<(), NodeError> {
        let repo = NodeProjectRepository::new(&self.pool);
        Ok(repo.delete_by_node_and_project(node_id, project_id).await?)
    }

    // =========================================================================
    // Task Assignment
    // =========================================================================

    /// Create a task assignment
    pub async fn assign_task(
        &self,
        task_id: Uuid,
        node_id: Uuid,
        node_project_id: Uuid,
    ) -> Result<NodeTaskAssignment, NodeError> {
        let repo = TaskAssignmentRepository::new(&self.pool);
        Ok(repo.create(task_id, node_id, node_project_id).await?)
    }

    /// Get the active assignment for a task
    pub async fn get_active_assignment(
        &self,
        task_id: Uuid,
    ) -> Result<Option<NodeTaskAssignment>, NodeError> {
        let repo = TaskAssignmentRepository::new(&self.pool);
        Ok(repo.find_active_for_task(task_id).await?)
    }

    /// List active assignments for a node
    pub async fn list_node_assignments(
        &self,
        node_id: Uuid,
    ) -> Result<Vec<NodeTaskAssignment>, NodeError> {
        let repo = TaskAssignmentRepository::new(&self.pool);
        Ok(repo.list_active_by_node(node_id).await?)
    }

    /// Update an assignment
    pub async fn update_assignment(
        &self,
        assignment_id: Uuid,
        data: UpdateAssignmentData,
    ) -> Result<NodeTaskAssignment, NodeError> {
        let repo = TaskAssignmentRepository::new(&self.pool);
        Ok(repo.update(assignment_id, data).await?)
    }

    /// Update assignment local IDs (task and attempt IDs from the node)
    pub async fn update_assignment_local_ids(
        &self,
        assignment_id: Uuid,
        local_task_id: Option<Uuid>,
        local_attempt_id: Option<Uuid>,
    ) -> Result<(), NodeError> {
        let data = UpdateAssignmentData {
            local_task_id,
            local_attempt_id,
            execution_status: None,
        };
        let repo = TaskAssignmentRepository::new(&self.pool);
        repo.update(assignment_id, data).await?;
        Ok(())
    }

    /// Update assignment execution status
    pub async fn update_assignment_status(
        &self,
        assignment_id: Uuid,
        status: &str,
    ) -> Result<(), NodeError> {
        let data = UpdateAssignmentData {
            local_task_id: None,
            local_attempt_id: None,
            execution_status: Some(status.to_string()),
        };
        let repo = TaskAssignmentRepository::new(&self.pool);
        repo.update(assignment_id, data).await?;
        Ok(())
    }

    /// Complete an assignment
    pub async fn complete_assignment(
        &self,
        assignment_id: Uuid,
        status: &str,
    ) -> Result<(), NodeError> {
        let repo = TaskAssignmentRepository::new(&self.pool);
        Ok(repo.complete(assignment_id, status).await?)
    }

    /// Fail all active assignments for a node (used when node goes offline)
    pub async fn fail_node_assignments(&self, node_id: Uuid) -> Result<Vec<Uuid>, NodeError> {
        let repo = TaskAssignmentRepository::new(&self.pool);
        Ok(repo.fail_node_assignments(node_id).await?)
    }
}

/// Trait for the node service (for testing/mocking)
pub trait NodeService: Send + Sync {
    fn create_api_key(
        &self,
        organization_id: Uuid,
        data: CreateNodeApiKey,
        created_by: Uuid,
    ) -> impl std::future::Future<Output = Result<(NodeApiKey, String), NodeError>> + Send;

    fn validate_api_key(
        &self,
        raw_key: &str,
    ) -> impl std::future::Future<Output = Result<NodeApiKey, NodeError>> + Send;

    fn register_node(
        &self,
        organization_id: Uuid,
        data: NodeRegistration,
    ) -> impl std::future::Future<Output = Result<Node, NodeError>> + Send;

    fn heartbeat(
        &self,
        node_id: Uuid,
        payload: HeartbeatPayload,
    ) -> impl std::future::Future<Output = Result<(), NodeError>> + Send;

    fn list_nodes(
        &self,
        organization_id: Uuid,
    ) -> impl std::future::Future<Output = Result<Vec<Node>, NodeError>> + Send;
}

impl NodeService for NodeServiceImpl {
    async fn create_api_key(
        &self,
        organization_id: Uuid,
        data: CreateNodeApiKey,
        created_by: Uuid,
    ) -> Result<(NodeApiKey, String), NodeError> {
        self.create_api_key(organization_id, data, created_by).await
    }

    async fn validate_api_key(&self, raw_key: &str) -> Result<NodeApiKey, NodeError> {
        self.validate_api_key(raw_key).await
    }

    async fn register_node(
        &self,
        organization_id: Uuid,
        data: NodeRegistration,
    ) -> Result<Node, NodeError> {
        self.register_node(organization_id, data).await
    }

    async fn heartbeat(&self, node_id: Uuid, payload: HeartbeatPayload) -> Result<(), NodeError> {
        self.heartbeat(node_id, payload).await
    }

    async fn list_nodes(&self, organization_id: Uuid) -> Result<Vec<Node>, NodeError> {
        self.list_nodes(organization_id).await
    }
}

/// Generate a random API key (32 bytes = 64 hex characters)
fn generate_api_key() -> String {
    let mut rng = rand::rng();
    let bytes: [u8; API_KEY_LEN] = rng.random();
    hex::encode(bytes)
}

/// Hash an API key using SHA256
fn hash_api_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_api_key_length() {
        let key = generate_api_key();
        assert_eq!(key.len(), API_KEY_LEN * 2); // hex encoding doubles length
    }

    #[test]
    fn test_hash_api_key_deterministic() {
        let key = "test_key_12345678901234567890123456789012";
        let hash1 = hash_api_key(key);
        let hash2 = hash_api_key(key);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_api_key_different_inputs() {
        let hash1 = hash_api_key("key1");
        let hash2 = hash_api_key("key2");
        assert_ne!(hash1, hash2);
    }
}
