//! Legacy project node types for backward compatibility.
//!
//! These types are kept for API compatibility after the migration from legacy
//! projects to swarm_projects. The actual project node queries now use
//! swarm_projects internally.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::nodes::NodeStatus;

/// Information about a node that has a project linked.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectNodeInfo {
    pub node_id: Uuid,
    pub node_name: String,
    pub node_status: NodeStatus,
    pub node_public_url: Option<String>,
    pub node_project_id: Uuid,
    pub local_project_id: Uuid,
}

/// Response containing a list of nodes that have a project linked.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListProjectNodesResponse {
    pub nodes: Vec<ProjectNodeInfo>,
}
