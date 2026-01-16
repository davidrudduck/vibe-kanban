//! Request and response types for projects routes.

use chrono::{DateTime, Utc};
use db::models::{cached_node::CachedNodeStatus, project::Project};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

// ============================================================================
// Query Parameters
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ListProjectFilesQuery {
    /// Relative path within the project (optional, defaults to root)
    pub path: Option<String>,
}

// ============================================================================
// Request Types
// ============================================================================

/// Request to link a local folder to a remote project
/// This creates a new local project at the specified path and links it to the remote project
#[derive(Deserialize, TS)]
pub struct LinkToLocalFolderRequest {
    /// The remote project ID to link to (from the Hive)
    pub remote_project_id: Uuid,
    /// The local folder path where the project will be created
    pub local_folder_path: String,
    /// Optional project name (defaults to folder name if not provided)
    pub project_name: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct OpenEditorRequest {
    pub editor_type: Option<String>,
}

/// Request to enable/disable GitHub integration for a project
#[derive(Debug, Deserialize, TS)]
pub struct SetGitHubEnabledRequest {
    pub enabled: bool,
    /// GitHub repository owner (e.g., "anthropics")
    pub owner: Option<String>,
    /// GitHub repository name (e.g., "claude-code")
    pub repo: Option<String>,
}

// ============================================================================
// Response Types
// ============================================================================

/// A project in the unified view - can be local or from another node
#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "type")]
#[allow(clippy::large_enum_variant)]
pub enum UnifiedProject {
    /// A local project on this node
    #[serde(rename = "local")]
    Local(Project),
    /// A project from another node (cached from hive sync)
    #[serde(rename = "remote")]
    Remote(RemoteNodeProject),
}

/// A project from another node in the organization (from unified projects table)
#[derive(Debug, Clone, Serialize, TS)]
pub struct RemoteNodeProject {
    /// Local ID in the unified projects table
    pub id: Uuid,
    /// ID of the node this project belongs to
    pub node_id: Uuid,
    /// Remote project ID from the Hive
    pub project_id: Uuid,
    pub project_name: String,
    pub git_repo_path: String,
    #[ts(type = "Date | null")]
    pub last_synced_at: Option<DateTime<Utc>>,
    #[ts(type = "Date")]
    pub created_at: DateTime<Utc>,
    // Node info
    pub node_name: String,
    pub node_status: CachedNodeStatus,
    pub node_public_url: Option<String>,
}

impl From<Project> for RemoteNodeProject {
    fn from(p: Project) -> Self {
        // Parse node status from the stored string
        let node_status = p
            .source_node_status
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(CachedNodeStatus::Pending);

        Self {
            id: p.id,
            node_id: p.source_node_id.unwrap_or_default(),
            project_id: p.remote_project_id.unwrap_or_default(),
            project_name: p.name,
            git_repo_path: p.git_repo_path.to_string_lossy().to_string(),
            last_synced_at: p.remote_last_synced_at,
            created_at: p.created_at,
            node_name: p.source_node_name.unwrap_or_default(),
            node_status,
            node_public_url: p.source_node_public_url,
        }
    }
}

/// A project in the merged view - merges local and remote projects by remote_project_id
#[derive(Debug, Clone, Serialize, TS)]
pub struct MergedProject {
    /// Use local project ID if exists, otherwise first remote's ID
    pub id: Uuid,
    pub name: String,
    pub git_repo_path: String,
    #[ts(type = "Date")]
    pub created_at: DateTime<Utc>,

    /// Linking status - Hive project ID (if linked)
    pub remote_project_id: Option<Uuid>,

    /// Location info - where the project runs
    pub has_local: bool,
    /// Local project ID if has_local is true
    pub local_project_id: Option<Uuid>,
    /// List of remote nodes that have this project
    pub nodes: Vec<NodeLocation>,

    /// For sorting - timestamp of last task attempt
    #[ts(type = "Date | null")]
    pub last_attempt_at: Option<DateTime<Utc>>,

    /// GitHub integration fields
    pub github_enabled: bool,
    pub github_owner: Option<String>,
    pub github_repo: Option<String>,
    pub github_open_issues: i32,
    pub github_open_prs: i32,
    #[ts(type = "Date | null")]
    pub github_last_synced_at: Option<DateTime<Utc>>,

    /// Task status counts for quick display
    pub task_counts: TaskCounts,
}

/// A node location where a project exists
#[derive(Debug, Clone, Serialize, TS)]
pub struct NodeLocation {
    pub node_id: Uuid,
    /// Full name like "tardis.raverx.net"
    pub node_name: String,
    /// Truncated at first period: "tardis"
    pub node_short_name: String,
    pub node_status: CachedNodeStatus,
    pub node_public_url: Option<String>,
    /// The project ID on that node
    pub remote_project_id: Uuid,
    /// Operating system: "darwin", "linux", "windows"
    pub node_os: Option<String>,
}

/// Task status counts for a project
#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export)]
pub struct TaskCounts {
    pub todo: i32,
    pub in_progress: i32,
    pub in_review: i32,
    pub done: i32,
}

/// Response for the merged projects endpoint
#[derive(Debug, Clone, Serialize, TS)]
pub struct MergedProjectsResponse {
    pub projects: Vec<MergedProject>,
}

/// A group of projects from a single remote node
#[derive(Debug, Clone, Serialize, TS)]
pub struct RemoteNodeGroup {
    pub node_id: Uuid,
    pub node_name: String,
    pub node_status: CachedNodeStatus,
    pub node_public_url: Option<String>,
    pub projects: Vec<RemoteNodeProject>,
}

/// Response for orphaned projects (projects with non-existent paths)
#[derive(Debug, Serialize, TS)]
pub struct OrphanedProject {
    pub id: Uuid,
    pub name: String,
    pub git_repo_path: String,
    pub is_remote: bool,
}

/// Response for orphaned projects list
#[derive(Debug, Serialize, TS)]
pub struct OrphanedProjectsResponse {
    pub projects: Vec<OrphanedProject>,
    pub count: usize,
}

#[derive(Debug, serde::Serialize, ts_rs::TS)]
pub struct OpenEditorResponse {
    pub url: Option<String>,
}

/// Response for GitHub counts
#[derive(Debug, Serialize, TS)]
pub struct GitHubCountsResponse {
    pub open_issues: i32,
    pub open_prs: i32,
    #[ts(type = "Date | null")]
    pub last_synced_at: Option<DateTime<Utc>>,
}
