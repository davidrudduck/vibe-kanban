//! WebSocket message types for node-hive communication.
//!
//! This module defines the protocol for bidirectional communication between
//! nodes (local instances) and the hive (remote server).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::nodes::domain::{NodeCapabilities, NodeStatus};

/// Messages sent from a node to the hive.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum NodeMessage {
    /// Initial authentication handshake
    #[serde(rename = "auth")]
    Auth(AuthMessage),

    /// Periodic heartbeat with status update
    #[serde(rename = "heartbeat")]
    Heartbeat(HeartbeatMessage),

    /// Task execution status update
    #[serde(rename = "task_status")]
    TaskStatus(TaskStatusMessage),

    /// Task execution output/logs
    #[serde(rename = "task_output")]
    TaskOutput(TaskOutputMessage),

    /// Task progress event (milestones)
    #[serde(rename = "task_progress")]
    TaskProgress(TaskProgressMessage),

    /// Link a project from the node to a remote project
    #[serde(rename = "link_project")]
    LinkProject(LinkProjectMessage),

    /// Unlink a project from the hive
    #[serde(rename = "unlink_project")]
    UnlinkProject(UnlinkProjectMessage),

    /// Sync a task attempt from node to hive
    #[serde(rename = "attempt_sync")]
    AttemptSync(AttemptSyncMessage),

    /// Sync an execution process from node to hive
    #[serde(rename = "execution_sync")]
    ExecutionSync(ExecutionSyncMessage),

    /// Batch of log entries from node to hive
    #[serde(rename = "logs_batch")]
    LogsBatch(LogsBatchMessage),

    /// Sync a label from node to hive
    #[serde(rename = "label_sync")]
    LabelSync(LabelSyncMessage),

    /// Sync a task from node to hive (create shared task for locally-started tasks)
    #[serde(rename = "task_sync")]
    TaskSync(TaskSyncMessage),

    /// Sync all local projects from node to hive
    #[serde(rename = "projects_sync")]
    ProjectsSync(ProjectsSyncMessage),

    /// Acknowledgement of a hive message
    #[serde(rename = "ack")]
    Ack { message_id: Uuid },

    /// Request to deregister this node from the hive
    #[serde(rename = "deregister")]
    Deregister(DeregisterMessage),

    /// Error response
    #[serde(rename = "error")]
    Error {
        message_id: Option<Uuid>,
        error: String,
    },
}

/// Messages sent from the hive to a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum HiveMessage {
    /// Authentication result
    #[serde(rename = "auth_result")]
    AuthResult(AuthResultMessage),

    /// Assign a task to the node
    #[serde(rename = "task_assign")]
    TaskAssign(TaskAssignMessage),

    /// Cancel a running task
    #[serde(rename = "task_cancel")]
    TaskCancel(TaskCancelMessage),

    /// Request immediate status update
    #[serde(rename = "status_request")]
    StatusRequest { message_id: Uuid },

    /// Sync project information
    #[serde(rename = "project_sync")]
    ProjectSync(ProjectSyncMessage),

    /// Heartbeat acknowledgement
    #[serde(rename = "heartbeat_ack")]
    HeartbeatAck { server_time: DateTime<Utc> },

    /// Error message
    #[serde(rename = "error")]
    Error {
        message_id: Option<Uuid>,
        error: String,
    },

    /// Connection closing
    #[serde(rename = "close")]
    Close { reason: String },

    /// Notification that a node has been removed from the organization
    #[serde(rename = "node_removed")]
    NodeRemoved(NodeRemovedMessage),

    /// Sync a label to node (broadcast from hive)
    #[serde(rename = "label_sync")]
    LabelSync(LabelSyncBroadcastMessage),

    /// Response to a task sync request from node
    #[serde(rename = "task_sync_response")]
    TaskSyncResponse(TaskSyncResponseMessage),
}

/// Authentication message from node to hive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthMessage {
    /// API key for authentication
    pub api_key: String,
    /// Unique machine identifier
    pub machine_id: String,
    /// Human-readable node name
    pub name: String,
    /// Node capabilities
    pub capabilities: NodeCapabilities,
    /// Public URL where this node can be reached (optional)
    pub public_url: Option<String>,
    /// Protocol version for compatibility
    pub protocol_version: u32,
}

/// Authentication result from hive to node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResultMessage {
    /// Whether authentication succeeded
    pub success: bool,
    /// Assigned node ID (if successful)
    pub node_id: Option<Uuid>,
    /// Organization ID (if successful)
    pub organization_id: Option<Uuid>,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Server's protocol version
    pub protocol_version: u32,
    /// Projects linked to this node
    pub linked_projects: Vec<LinkedProjectInfo>,
    /// Swarm labels for the organization (synced from hive to nodes)
    #[serde(default)]
    pub swarm_labels: Vec<SwarmLabelInfo>,
}

/// Information about a swarm label sent during auth.
///
/// Swarm labels are organization-global labels (project_id = NULL) that are
/// managed on the hive and synced to nodes. These labels should be used
/// for tasks in swarm-connected projects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmLabelInfo {
    /// Label ID on the hive
    pub id: Uuid,
    /// Label name
    pub name: String,
    /// Lucide icon name
    pub icon: String,
    /// Hex color code
    pub color: String,
    /// Version for conflict resolution
    pub version: i64,
}

/// Information about a project in the organization sent during auth.
///
/// This includes both projects owned by this node and projects owned by other nodes.
/// The `is_owned` field indicates whether this node owns the project (has the git repo).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedProjectInfo {
    /// Link ID (for owned projects) or a generated UUID (for visible-only projects)
    pub link_id: Uuid,
    /// Remote project ID in the hive's projects table
    pub project_id: Uuid,
    /// Local project ID on the owning node
    pub local_project_id: Uuid,
    /// Git repository path on the owning node
    pub git_repo_path: String,
    /// Default branch for the project
    pub default_branch: String,
    /// Project name for display
    pub project_name: String,
    /// ID of the node that owns this project (has the git repo)
    pub source_node_id: Uuid,
    /// Name of the node that owns this project
    pub source_node_name: String,
    /// Whether this node owns the project (true) or just has visibility (false)
    pub is_owned: bool,
}

/// Heartbeat message from node to hive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatMessage {
    /// Current node status
    pub status: NodeStatus,
    /// Number of tasks currently executing
    pub active_tasks: u32,
    /// Available executor capacity
    pub available_capacity: u32,
    /// Memory usage percentage (0-100)
    pub memory_usage: Option<u8>,
    /// CPU usage percentage (0-100)
    pub cpu_usage: Option<u8>,
    /// Timestamp from the node
    pub timestamp: DateTime<Utc>,
}

/// Task assignment from hive to node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAssignMessage {
    /// Unique message ID for acknowledgement
    pub message_id: Uuid,
    /// Assignment ID in the hive database
    pub assignment_id: Uuid,
    /// Task ID in the hive database
    pub task_id: Uuid,
    /// Node project link ID
    pub node_project_id: Uuid,
    /// Local project ID on the node
    pub local_project_id: Uuid,
    /// Task details
    pub task: TaskDetails,
}

/// Task details sent with assignment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDetails {
    pub title: String,
    pub description: Option<String>,
    pub executor: String,
    pub executor_variant: Option<String>,
    pub base_branch: String,
}

/// Task cancellation request from hive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCancelMessage {
    /// Unique message ID for acknowledgement
    pub message_id: Uuid,
    /// Assignment ID to cancel
    pub assignment_id: Uuid,
    /// Reason for cancellation
    pub reason: Option<String>,
}

/// Task status update from node to hive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusMessage {
    /// Assignment ID
    pub assignment_id: Uuid,
    /// Local task ID on the node
    pub local_task_id: Option<Uuid>,
    /// Local attempt ID on the node
    pub local_attempt_id: Option<Uuid>,
    /// Current execution status
    pub status: TaskExecutionStatus,
    /// Status message/details
    pub message: Option<String>,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Task execution status values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskExecutionStatus {
    /// Task received and queued
    Pending,
    /// Task is starting
    Starting,
    /// Task is running
    Running,
    /// Task completed successfully
    Completed,
    /// Task failed
    Failed,
    /// Task was cancelled
    Cancelled,
}

/// Task output/log stream from node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskOutputMessage {
    /// Assignment ID
    pub assignment_id: Uuid,
    /// Output type
    pub output_type: TaskOutputType,
    /// Output content
    pub content: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Type of task output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskOutputType {
    Stdout,
    Stderr,
    System,
}

/// Task progress event from node to hive.
///
/// Progress events represent significant milestones during task execution,
/// such as agent startup, PR creation, or branch pushes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgressMessage {
    /// Assignment ID
    pub assignment_id: Uuid,
    /// Type of progress event
    pub event_type: TaskProgressType,
    /// Optional message/description
    pub message: Option<String>,
    /// Optional metadata (e.g., PR URL, branch name)
    pub metadata: Option<serde_json::Value>,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Type of task progress event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskProgressType {
    /// Agent has started processing
    AgentStarted,
    /// Agent is thinking/planning
    AgentThinking,
    /// Code changes being made
    CodeChanges,
    /// Branch has been created
    BranchCreated,
    /// Changes have been committed
    Committed,
    /// Branch has been pushed
    Pushed,
    /// Pull request created
    PullRequestCreated,
    /// Agent finished (use status update for success/failure)
    AgentFinished,
    /// Custom milestone
    Custom,
}

/// Link a project from node to hive.
///
/// Sent by the node when a user links a local project to a remote project.
/// This creates an entry in the hive's node_projects table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkProjectMessage {
    /// The remote project ID (from the hive's projects table)
    pub project_id: Uuid,
    /// The local project ID on the node
    pub local_project_id: Uuid,
    /// Path to the git repository on the node
    pub git_repo_path: String,
    /// Default branch for the project
    #[serde(default = "default_branch")]
    pub default_branch: String,
}

fn default_branch() -> String {
    "main".to_string()
}

/// Unlink a project from the hive.
///
/// Sent by the node when a user unlinks a local project.
/// This removes the entry from the hive's node_projects table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnlinkProjectMessage {
    /// The remote project ID to unlink
    pub project_id: Uuid,
}

/// Request to deregister a node from the hive.
///
/// Sent by the node when disconnecting from the swarm.
/// This performs a hard delete of all node data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeregisterMessage {
    /// Unique message ID for acknowledgement
    pub message_id: Uuid,
    /// Optional reason for deregistering
    pub reason: Option<String>,
}

/// Notification that a node has been removed from the organization.
///
/// Broadcast to all nodes in the organization when a node deregisters
/// or is deleted by an admin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRemovedMessage {
    /// ID of the removed node
    pub node_id: Uuid,
    /// Reason for removal
    pub reason: String,
}

/// Project sync message from hive to node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSyncMessage {
    /// Unique message ID for acknowledgement
    pub message_id: Uuid,
    /// Link ID
    pub link_id: Uuid,
    /// Project ID in the hive
    pub project_id: Uuid,
    /// Project name
    pub project_name: String,
    /// Local project ID on the node
    pub local_project_id: Uuid,
    /// Git repository path
    pub git_repo_path: String,
    /// Default branch
    pub default_branch: String,
    /// Source node ID (which node owns this project)
    pub source_node_id: Uuid,
    /// Source node name
    pub source_node_name: String,
    /// Source node public URL for direct proxy
    pub source_node_public_url: Option<String>,
    /// Whether this is a new link (true) or removal (false)
    pub is_new: bool,
}

/// Sync a task attempt from node to hive.
///
/// Sent by nodes when a task attempt is created or updated.
/// The hive stores this in node_task_attempts for tracking execution history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptSyncMessage {
    /// Local attempt ID (same as node's task_attempt.id)
    pub attempt_id: Uuid,
    /// Assignment ID if this attempt was dispatched via hive
    pub assignment_id: Option<Uuid>,
    /// Shared task ID in the hive
    pub shared_task_id: Uuid,
    /// Executor name (e.g., "CLAUDE_CODE", "GEMINI")
    pub executor: String,
    /// Executor variant (e.g., "opus", "sonnet")
    pub executor_variant: Option<String>,
    /// Git branch for this attempt
    pub branch: String,
    /// Target branch for PR/merge
    pub target_branch: String,
    /// Container reference (worktree path or container ID)
    pub container_ref: Option<String>,
    /// Whether the worktree has been deleted
    pub worktree_deleted: bool,
    /// When setup completed (if applicable)
    pub setup_completed_at: Option<DateTime<Utc>>,
    /// When the attempt was created on the node
    pub created_at: DateTime<Utc>,
    /// When the attempt was last updated on the node
    pub updated_at: DateTime<Utc>,
}

/// Sync an execution process from node to hive.
///
/// Sent by nodes when an execution process is created or updated.
/// The hive stores this in node_execution_processes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSyncMessage {
    /// Local execution process ID (same as node's execution_process.id)
    pub execution_id: Uuid,
    /// Attempt ID this process belongs to
    pub attempt_id: Uuid,
    /// Run reason (setupscript, cleanupscript, codingagent, devserver)
    pub run_reason: String,
    /// Executor action details (JSON)
    pub executor_action: Option<serde_json::Value>,
    /// Git HEAD before process ran
    pub before_head_commit: Option<String>,
    /// Git HEAD after process completed
    pub after_head_commit: Option<String>,
    /// Process status (running, completed, failed, killed)
    pub status: String,
    /// Exit code if completed
    pub exit_code: Option<i32>,
    /// Whether this process is dropped from timeline view
    pub dropped: bool,
    /// System process ID
    pub pid: Option<i64>,
    /// When the process started
    pub started_at: DateTime<Utc>,
    /// When the process completed
    pub completed_at: Option<DateTime<Utc>>,
    /// When the process record was created
    pub created_at: DateTime<Utc>,
}

/// A single log entry in a batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Output type (stdout, stderr, system)
    pub output_type: TaskOutputType,
    /// Log content
    pub content: String,
    /// Timestamp of the log entry
    pub timestamp: DateTime<Utc>,
}

/// Batch of log entries from node to hive.
///
/// Nodes batch log entries for efficiency (typically 100 entries or 5 seconds).
/// The execution_process_id links logs to a specific process run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogsBatchMessage {
    /// Assignment ID for routing
    pub assignment_id: Uuid,
    /// Execution process ID these logs belong to (optional for backwards compatibility)
    pub execution_process_id: Option<Uuid>,
    /// Batch of log entries
    pub entries: Vec<LogEntry>,
    /// Whether this batch is compressed (gzip)
    #[serde(default)]
    pub compressed: bool,
}

/// Sync a label from node to hive.
///
/// Sent by nodes when a label is created or updated.
/// The hive stores this in the labels table and broadcasts to other nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelSyncMessage {
    /// The local label ID on the node
    pub label_id: Uuid,
    /// The shared label ID on the Hive (None for new labels)
    pub shared_label_id: Option<Uuid>,
    /// The project ID if this is a project-specific label, None for global labels
    pub project_id: Option<Uuid>,
    /// Remote project ID if the label is project-specific (for linking on hive side)
    pub remote_project_id: Option<Uuid>,
    /// Label name
    pub name: String,
    /// Lucide icon name
    pub icon: String,
    /// Hex color code (e.g., "#3b82f6")
    pub color: String,
    /// Version for conflict resolution
    pub version: i64,
    /// Whether this is an update to an existing label (vs new label)
    pub is_update: bool,
}

/// Label sync broadcast from hive to nodes.
///
/// Sent by the hive when a label is created or updated on another node.
/// Nodes use this to keep their local label cache in sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelSyncBroadcastMessage {
    /// Unique message ID for acknowledgement
    pub message_id: Uuid,
    /// The shared label ID on the Hive
    pub shared_label_id: Uuid,
    /// The project ID if this is a project-specific label, None for global labels
    pub project_id: Option<Uuid>,
    /// ID of the node that owns/created this label
    pub origin_node_id: Uuid,
    /// Label name
    pub name: String,
    /// Lucide icon name
    pub icon: String,
    /// Hex color code (e.g., "#3b82f6")
    pub color: String,
    /// Version for conflict resolution
    pub version: i64,
    /// Whether this label was deleted
    pub is_deleted: bool,
}

/// Sync a task from node to hive.
///
/// Sent by nodes to create or update a shared task on the hive.
/// This allows locally-started tasks to be visible across the swarm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSyncMessage {
    /// The local task ID on the node
    pub local_task_id: Uuid,
    /// The shared task ID on the Hive (None for new tasks)
    pub shared_task_id: Option<Uuid>,
    /// The remote project ID (required for task creation)
    pub remote_project_id: Uuid,
    /// Task title
    pub title: String,
    /// Task description
    pub description: Option<String>,
    /// Task status (todo, in_progress, in_review, done, cancelled)
    pub status: String,
    /// Version for conflict resolution
    pub version: i64,
    /// Whether this is an update to an existing task (vs new task)
    pub is_update: bool,
    /// When the task was created locally
    pub created_at: DateTime<Utc>,
    /// When the task was last updated locally
    pub updated_at: DateTime<Utc>,
}

/// Response to a task sync request.
///
/// Sent by the hive to confirm the shared_task_id for a synced task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSyncResponseMessage {
    /// The local task ID on the node (echoed back for correlation)
    pub local_task_id: Uuid,
    /// The shared task ID on the Hive
    pub shared_task_id: Uuid,
    /// Whether the operation was successful
    pub success: bool,
    /// Error message if not successful
    pub error: Option<String>,
}

/// Sync all local projects from node to hive.
///
/// Sent by nodes on connection and periodically to keep the hive's
/// `node_local_projects` table up to date. This enables the swarm settings
/// UI to show all projects from all nodes for linking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectsSyncMessage {
    /// All local projects on this node
    pub projects: Vec<LocalProjectInfo>,
}

/// Information about a local project from a node.
///
/// Used in ProjectsSyncMessage to sync project info to the hive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalProjectInfo {
    /// The local project ID on the node
    pub local_project_id: Uuid,
    /// Project name
    pub name: String,
    /// Git repository path on the node
    pub git_repo_path: String,
    /// Default branch for the project
    pub default_branch: String,
}

/// Current protocol version.
pub const PROTOCOL_VERSION: u32 = 1;
