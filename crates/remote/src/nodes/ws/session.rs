//! WebSocket session handler for node connections.
//!
//! This module handles the lifecycle of a single node WebSocket connection,
//! including authentication, message routing, and heartbeat management.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use chrono::Utc;
use futures::{SinkExt, StreamExt};
use sqlx::PgPool;
use tokio::{
    sync::mpsc,
    time::{self, MissedTickBehavior},
};
use tracing::{Span, instrument};
use uuid::Uuid;

use super::{
    connection::ConnectionManager,
    message::{
        AttemptSyncMessage, AuthResultMessage, BackfillResponseMessage, DeregisterMessage,
        ExecutionSyncMessage, HeartbeatMessage, HiveMessage, LinkProjectMessage, LinkedProjectInfo,
        LogsBatchMessage, NodeMessage, NodeRemovedMessage, PROTOCOL_VERSION, ProjectSyncMessage,
        ProjectsSyncMessage, SwarmLabelInfo, TaskExecutionStatus, TaskOutputMessage,
        TaskProgressMessage, TaskStatusMessage, TaskSyncMessage, TaskSyncResponseMessage,
        UnlinkProjectMessage,
    },
};
use crate::{
    db::node_task_attempts::NodeTaskAttemptRepository,
    nodes::{
        BackfillService,
        backfill::BackfillRequestTracker,
        domain::NodeStatus,
        service::{NodeServiceImpl, RegisterNode},
    },
};

/// Heartbeat timeout - close connection if no heartbeat received.
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(90);

/// Channel buffer size for outgoing messages.
const OUTGOING_BUFFER_SIZE: usize = 64;

/// Returns the final path component of a git repository path.
///
/// Trims trailing '/' and '\' characters and supports both Unix and Windows separators. If the input contains no separators, or is empty, the original input is returned.
///
/// # Examples
///
/// ```
/// assert_eq!(extract_project_name("https://example.com/org/repo.git"), "repo.git");
/// assert_eq!(extract_project_name("C:\\path\\to\\project\\"), "project");
/// assert_eq!(extract_project_name("/single_component"), "single_component");
/// assert_eq!(extract_project_name(""), "");
/// ```
fn extract_project_name(git_repo_path: &str) -> String {
    let trimmed = git_repo_path.trim_end_matches(['/', '\\']);
    let candidate = if trimmed.is_empty() {
        git_repo_path
    } else {
        trimmed
    };
    candidate
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(candidate)
        .to_string()
}

/// Handle a new node WebSocket connection.
#[instrument(
    name = "node_ws.session",
    skip(socket, pool, connections, backfill),
    fields(
        node_id = tracing::field::Empty,
        org_id = tracing::field::Empty,
        machine_id = tracing::field::Empty
    )
)]
pub async fn handle(
    socket: WebSocket,
    pool: PgPool,
    connections: ConnectionManager,
    backfill: Arc<BackfillService>,
) {
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let (tx, mut rx) = mpsc::channel::<HiveMessage>(OUTGOING_BUFFER_SIZE);

    // Wait for authentication message
    let auth_result = match wait_for_auth(&mut ws_receiver, &pool).await {
        Ok(result) => result,
        Err(error) => {
            tracing::info!(?error, "node authentication failed");
            let _ = send_message(
                &mut ws_sender,
                &HiveMessage::AuthResult(AuthResultMessage {
                    success: false,
                    node_id: None,
                    organization_id: None,
                    error: Some(error.to_string()),
                    protocol_version: PROTOCOL_VERSION,
                    linked_projects: vec![],
                    swarm_labels: vec![],
                }),
            )
            .await;
            return;
        }
    };

    // Record context in span
    Span::current().record("node_id", format_args!("{}", auth_result.node_id));
    Span::current().record("org_id", format_args!("{}", auth_result.organization_id));

    // Clone projects for broadcast before moving into response
    let projects_for_broadcast = auth_result.linked_projects.clone();

    tracing::info!(
        node_id = %auth_result.node_id,
        org_id = %auth_result.organization_id,
        linked_projects = auth_result.linked_projects.len(),
        swarm_labels = auth_result.swarm_labels.len(),
        "sending auth success with swarm labels"
    );

    // Send auth success response
    if send_message(
        &mut ws_sender,
        &HiveMessage::AuthResult(AuthResultMessage {
            success: true,
            node_id: Some(auth_result.node_id),
            organization_id: Some(auth_result.organization_id),
            error: None,
            protocol_version: PROTOCOL_VERSION,
            linked_projects: auth_result.linked_projects,
            swarm_labels: auth_result.swarm_labels,
        }),
    )
    .await
    .is_err()
    {
        return;
    }

    // Register connection
    connections
        .register(auth_result.node_id, auth_result.organization_id, tx)
        .await;

    // Broadcast this node's projects to other nodes in the organization
    broadcast_node_projects(
        auth_result.node_id,
        auth_result.organization_id,
        &auth_result.node_name,
        auth_result.node_public_url.as_deref(),
        &projects_for_broadcast,
        &pool,
        &connections,
    )
    .await;

    // Trigger backfill for incomplete attempts (non-blocking)
    let node_id_for_backfill = auth_result.node_id;
    let backfill_service = Arc::clone(&backfill);
    tokio::spawn(async move {
        match backfill_service
            .trigger_reconnect_backfill(node_id_for_backfill)
            .await
        {
            Ok(count) => {
                if count > 0 {
                    tracing::info!(
                        node_id = %node_id_for_backfill,
                        count = count,
                        "triggered backfill for incomplete attempts on reconnect"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    node_id = %node_id_for_backfill,
                    error = %e,
                    "failed to trigger backfill on reconnect"
                );
            }
        }
    });

    // Set up heartbeat timeout
    let mut heartbeat_timeout = time::interval(HEARTBEAT_TIMEOUT);
    heartbeat_timeout.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut last_heartbeat = Utc::now();

    tracing::info!(
        node_id = %auth_result.node_id,
        organization_id = %auth_result.organization_id,
        "node session started"
    );

    // Get tracker for correlating backfill responses
    let tracker = backfill.tracker();

    // Main message loop
    loop {
        tokio::select! {
            // Handle outgoing messages from hive
            Some(msg) = rx.recv() => {
                if send_message(&mut ws_sender, &msg).await.is_err() {
                    break;
                }
            }

            // Handle incoming messages from node
            maybe_message = ws_receiver.next() => {
                match maybe_message {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<NodeMessage>(&text) {
                            Ok(msg) => {
                                if let Err(error) = handle_node_message(
                                    &msg,
                                    auth_result.node_id,
                                    auth_result.organization_id,
                                    &pool,
                                    &connections,
                                    &mut ws_sender,
                                    &mut last_heartbeat,
                                    &tracker,
                                ).await {
                                    tracing::warn!(?error, "error handling node message");
                                }
                            }
                            Err(error) => {
                                tracing::debug!(?error, "invalid node message");
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        tracing::debug!("node sent close frame");
                        break;
                    }
                    Some(Ok(Message::Ping(data))) => {
                        if ws_sender.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(_)) => {
                        // Ignore other message types
                    }
                    Some(Err(error)) => {
                        tracing::debug!(?error, "websocket receive error");
                        break;
                    }
                    None => break,
                }
            }

            // Check heartbeat timeout
            _ = heartbeat_timeout.tick() => {
                let elapsed = Utc::now().signed_duration_since(last_heartbeat);
                if elapsed > chrono::Duration::from_std(HEARTBEAT_TIMEOUT).unwrap() {
                    tracing::warn!(
                        node_id = %auth_result.node_id,
                        elapsed_secs = elapsed.num_seconds(),
                        "node heartbeat timeout"
                    );
                    let _ = send_message(
                        &mut ws_sender,
                        &HiveMessage::Close {
                            reason: "heartbeat timeout".to_string(),
                        },
                    ).await;
                    break;
                }
            }
        }
    }

    // Clean up
    connections.unregister(auth_result.node_id).await;

    // Clear any pending backfill requests for this node and reset attempts to partial
    let cleared_attempt_ids = tracker.clear_node(auth_result.node_id).await;
    if !cleared_attempt_ids.is_empty() {
        tracing::info!(
            node_id = %auth_result.node_id,
            count = cleared_attempt_ids.len(),
            "clearing pending backfill requests on disconnect"
        );

        let repo = NodeTaskAttemptRepository::new(&pool);
        for attempt_id in cleared_attempt_ids {
            if let Err(e) = repo.reset_attempt_to_partial(attempt_id).await {
                tracing::warn!(
                    node_id = %auth_result.node_id,
                    attempt_id = %attempt_id,
                    error = %e,
                    "failed to reset attempt to partial on disconnect"
                );
            }
        }
    }

    // Update node status to offline
    let service = NodeServiceImpl::new(pool.clone());
    if let Err(error) = service
        .update_node_status(auth_result.node_id, NodeStatus::Offline)
        .await
    {
        tracing::warn!(?error, "failed to update node status to offline");
    }

    tracing::info!(
        node_id = %auth_result.node_id,
        "node session ended"
    );
}

/// Result of successful authentication.
struct AuthResult {
    node_id: Uuid,
    organization_id: Uuid,
    node_name: String,
    node_public_url: Option<String>,
    linked_projects: Vec<LinkedProjectInfo>,
    /// Swarm labels for the organization (synced to nodes on connect)
    swarm_labels: Vec<SwarmLabelInfo>,
}

/// Authentication error.
#[derive(Debug, thiserror::Error)]
enum AuthError {
    #[error("timeout waiting for auth message")]
    Timeout,
    #[error("connection closed before auth")]
    ConnectionClosed,
    #[error("invalid auth message: {0}")]
    InvalidMessage(String),
    #[error("invalid API key")]
    InvalidApiKey,
    #[error("API key revoked")]
    ApiKeyRevoked,
    #[error("API key blocked: {0}")]
    ApiKeyBlocked(String),
    #[error("takeover detected: {0}")]
    TakeoverDetected(String),
    #[error("protocol version mismatch: client={client}, server={server}")]
    ProtocolMismatch { client: u32, server: u32 },
    #[error("registration failed: {0}")]
    RegistrationFailed(String),
}

/// Wait for and process the authentication message.
async fn wait_for_auth(
    receiver: &mut futures::stream::SplitStream<WebSocket>,
    pool: &PgPool,
) -> Result<AuthResult, AuthError> {
    // Wait for auth message with timeout
    let auth_timeout = Duration::from_secs(30);
    let message = tokio::time::timeout(auth_timeout, receiver.next())
        .await
        .map_err(|_| AuthError::Timeout)?
        .ok_or(AuthError::ConnectionClosed)?
        .map_err(|e| AuthError::InvalidMessage(e.to_string()))?;

    let text = match message {
        Message::Text(text) => text,
        _ => {
            return Err(AuthError::InvalidMessage(
                "expected text message".to_string(),
            ));
        }
    };

    let node_msg: NodeMessage =
        serde_json::from_str(&text).map_err(|e| AuthError::InvalidMessage(e.to_string()))?;

    let auth = match node_msg {
        NodeMessage::Auth(auth) => auth,
        _ => {
            return Err(AuthError::InvalidMessage(
                "expected auth message".to_string(),
            ));
        }
    };

    Span::current().record("machine_id", &auth.machine_id);

    // Validate protocol version
    if auth.protocol_version != PROTOCOL_VERSION {
        return Err(AuthError::ProtocolMismatch {
            client: auth.protocol_version,
            server: PROTOCOL_VERSION,
        });
    }

    // Validate API key and get organization
    let service = NodeServiceImpl::new(pool.clone());
    let api_key = service.validate_api_key(&auth.api_key).await.map_err(|e| {
        tracing::debug!(?e, "API key validation failed");
        match e {
            crate::nodes::service::NodeError::ApiKeyRevoked => AuthError::ApiKeyRevoked,
            crate::nodes::service::NodeError::ApiKeyBlocked(reason) => {
                AuthError::ApiKeyBlocked(reason)
            }
            _ => AuthError::InvalidApiKey,
        }
    })?;

    // Check if API key is blocked (additional check after validation)
    if let Some(reason) = &api_key.blocked_reason {
        tracing::info!(
            key_id = %api_key.id,
            key_name = %api_key.name,
            reason = %reason,
            "Rejecting blocked API key"
        );
        return Err(AuthError::ApiKeyBlocked(reason.clone()));
    }

    // Store name and public_url for later use in broadcasts
    let node_name = auth.name.clone();
    let node_public_url = auth.public_url.clone();

    // Register or update the node using API key-based identity
    let register_data = RegisterNode {
        name: auth.name,
        machine_id: auth.machine_id,
        capabilities: auth.capabilities,
        public_url: auth.public_url,
    };

    let node = service
        .register_node_with_api_key(&api_key, register_data)
        .await
        .map_err(|e| {
            tracing::debug!(?e, "Node registration failed");
            match e {
                crate::nodes::service::NodeError::TakeoverDetected(msg) => {
                    AuthError::TakeoverDetected(msg)
                }
                crate::nodes::service::NodeError::ApiKeyBlocked(reason) => {
                    AuthError::ApiKeyBlocked(reason)
                }
                _ => AuthError::RegistrationFailed(e.to_string()),
            }
        })?;

    // Get only swarm projects this node is linked to (not all org projects)
    // This prevents visibility leak where nodes see other nodes' unlinked projects
    use crate::db::swarm_projects::SwarmProjectRepository;
    let swarm_projects = SwarmProjectRepository::list_for_node_auth(pool, node.id)
        .await
        .map_err(|e| AuthError::RegistrationFailed(e.to_string()))?;

    // Convert to LinkedProjectInfo with ownership info
    // In the new swarm architecture, all returned projects are "owned" by this node
    // since we only return projects the node is linked to
    let linked_projects = swarm_projects
        .into_iter()
        .map(|p| LinkedProjectInfo {
            link_id: p.link_id,
            project_id: p.swarm_project_id, // Use swarm_project_id as project_id
            local_project_id: p.local_project_id,
            git_repo_path: p.git_repo_path,
            default_branch: p.default_branch,
            project_name: p.project_name,
            source_node_id: p.source_node_id,
            source_node_name: p.source_node_name,
            is_owned: true, // Node is always linked to projects it receives
        })
        .collect();

    // Fetch swarm labels for the organization (org-global labels with project_id = NULL)
    // These are synced to nodes on connection so they can use them for swarm tasks
    let swarm_labels = {
        use crate::db::labels::LabelRepository;
        let label_repo = LabelRepository::new(pool);
        label_repo
            .find_swarm_labels(node.organization_id)
            .await
            .map_err(|e| AuthError::RegistrationFailed(format!("failed to fetch labels: {}", e)))?
            .into_iter()
            .map(|l| SwarmLabelInfo {
                id: l.id,
                name: l.name,
                icon: l.icon,
                color: l.color,
                version: l.version,
            })
            .collect()
    };

    Ok(AuthResult {
        node_id: node.id,
        organization_id: node.organization_id,
        node_name,
        node_public_url,
        linked_projects,
        swarm_labels,
    })
}

/// Handle an incoming message from a node.
#[allow(clippy::too_many_arguments)]
async fn handle_node_message(
    msg: &NodeMessage,
    node_id: Uuid,
    organization_id: Uuid,
    pool: &PgPool,
    connections: &ConnectionManager,
    ws_sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    last_heartbeat: &mut chrono::DateTime<Utc>,
    tracker: &BackfillRequestTracker,
) -> Result<(), HandleError> {
    match msg {
        NodeMessage::Heartbeat(heartbeat) => {
            handle_heartbeat(
                node_id,
                heartbeat,
                pool,
                connections,
                ws_sender,
                last_heartbeat,
            )
            .await
        }
        NodeMessage::TaskStatus(status) => {
            handle_task_status(node_id, organization_id, status, pool).await
        }
        NodeMessage::TaskOutput(output) => handle_task_output(node_id, output, pool).await,
        NodeMessage::TaskProgress(progress) => handle_task_progress(node_id, progress, pool).await,
        NodeMessage::LinkProject(link) => {
            handle_link_project(node_id, organization_id, link, pool, connections).await
        }
        NodeMessage::UnlinkProject(unlink) => {
            handle_unlink_project(node_id, organization_id, unlink, pool, connections).await
        }
        NodeMessage::Deregister(deregister) => {
            handle_deregister(node_id, organization_id, deregister, pool, connections).await
        }
        NodeMessage::AttemptSync(attempt) => handle_attempt_sync(node_id, attempt, pool).await,
        NodeMessage::ExecutionSync(execution) => {
            handle_execution_sync(node_id, execution, pool).await
        }
        NodeMessage::LogsBatch(logs) => handle_logs_batch(node_id, logs, pool).await,
        NodeMessage::LabelSync(_label) => {
            // Labels are no longer synced from nodes to hive.
            // Labels are managed centrally on the hive and synced DOWN to nodes.
            // Ignore incoming label sync messages from nodes.
            tracing::debug!(
                node_id = %node_id,
                "ignoring deprecated label sync from node - labels are now hive-managed"
            );
            Ok(())
        }
        NodeMessage::TaskSync(task) => {
            handle_task_sync(node_id, organization_id, task, pool, ws_sender).await
        }
        NodeMessage::ProjectsSync(projects) => handle_projects_sync(node_id, projects, pool).await,
        NodeMessage::Ack { message_id } => {
            tracing::trace!(node_id = %node_id, message_id = %message_id, "received ack");
            Ok(())
        }
        NodeMessage::Error { message_id, error } => {
            tracing::warn!(
                node_id = %node_id,
                message_id = ?message_id,
                error = %error,
                "received error from node"
            );
            Ok(())
        }
        NodeMessage::Auth(_) => {
            // Auth should only happen once at connection start
            tracing::warn!(node_id = %node_id, "received unexpected auth message");
            Ok(())
        }
        NodeMessage::BackfillResponse(response) => {
            handle_backfill_response(node_id, response, pool, tracker).await
        }
    }
}

/// Handle a heartbeat message.
async fn handle_heartbeat(
    node_id: Uuid,
    heartbeat: &HeartbeatMessage,
    pool: &PgPool,
    connections: &ConnectionManager,
    ws_sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    last_heartbeat: &mut chrono::DateTime<Utc>,
) -> Result<(), HandleError> {
    *last_heartbeat = Utc::now();

    // Update connection status
    connections
        .update_status(node_id, heartbeat.status, heartbeat.active_tasks)
        .await;

    // Update database
    let service = NodeServiceImpl::new(pool.clone());
    service
        .update_node_status(node_id, heartbeat.status)
        .await
        .map_err(|e| HandleError::Database(e.to_string()))?;

    // Send heartbeat acknowledgement
    send_message(
        ws_sender,
        &HiveMessage::HeartbeatAck {
            server_time: Utc::now(),
        },
    )
    .await
    .map_err(|_| HandleError::Send)?;

    tracing::trace!(
        node_id = %node_id,
        status = ?heartbeat.status,
        active_tasks = heartbeat.active_tasks,
        "processed heartbeat"
    );

    Ok(())
}

/// Handle a task status update.
async fn handle_task_status(
    node_id: Uuid,
    _organization_id: Uuid,
    status: &TaskStatusMessage,
    pool: &PgPool,
) -> Result<(), HandleError> {
    use crate::db::task_assignments::TaskAssignmentRepository;
    use crate::db::tasks::{SharedTaskRepository, TaskStatus};

    let service = NodeServiceImpl::new(pool.clone());

    // Map execution status to database status string
    let db_status = match status.status {
        TaskExecutionStatus::Pending => "pending",
        TaskExecutionStatus::Starting => "starting",
        TaskExecutionStatus::Running => "running",
        TaskExecutionStatus::Completed => "completed",
        TaskExecutionStatus::Failed => "failed",
        TaskExecutionStatus::Cancelled => "cancelled",
    };

    // Update local IDs if provided
    if status.local_task_id.is_some() || status.local_attempt_id.is_some() {
        service
            .update_assignment_local_ids(
                status.assignment_id,
                status.local_task_id,
                status.local_attempt_id,
            )
            .await
            .map_err(|e| HandleError::Database(e.to_string()))?;
    }

    // Update execution status on the assignment
    service
        .update_assignment_status(status.assignment_id, db_status)
        .await
        .map_err(|e| HandleError::Database(e.to_string()))?;

    // Also update the shared task status
    // First, get the task_id from the assignment
    let assignment_repo = TaskAssignmentRepository::new(pool);
    if let Ok(Some(assignment)) = assignment_repo.find_by_id(status.assignment_id).await {
        // Map execution status to shared task status
        let shared_status = match status.status {
            TaskExecutionStatus::Pending | TaskExecutionStatus::Starting => TaskStatus::Todo,
            TaskExecutionStatus::Running => TaskStatus::InProgress,
            TaskExecutionStatus::Completed => TaskStatus::InReview,
            TaskExecutionStatus::Failed | TaskExecutionStatus::Cancelled => TaskStatus::Todo,
        };

        let task_repo = SharedTaskRepository::new(pool);
        if let Err(e) = task_repo
            .update_status_from_node(assignment.task_id, shared_status)
            .await
        {
            tracing::warn!(
                task_id = %assignment.task_id,
                error = %e,
                "failed to update shared task status"
            );
        }
    }

    tracing::info!(
        node_id = %node_id,
        assignment_id = %status.assignment_id,
        status = ?status.status,
        "task status updated"
    );

    Ok(())
}

/// Handle task output/log messages from a node.
async fn handle_task_output(
    node_id: Uuid,
    output: &TaskOutputMessage,
    pool: &PgPool,
) -> Result<(), HandleError> {
    use crate::db::task_output_logs::{CreateTaskOutputLog, TaskOutputLogRepository};

    let output_type = match output.output_type {
        super::message::TaskOutputType::Stdout => "stdout",
        super::message::TaskOutputType::Stderr => "stderr",
        super::message::TaskOutputType::System => "system",
    };

    let repo = TaskOutputLogRepository::new(pool);
    repo.create(CreateTaskOutputLog {
        assignment_id: output.assignment_id,
        output_type: output_type.to_string(),
        content: output.content.clone(),
        timestamp: output.timestamp,
    })
    .await
    .map_err(|e| HandleError::Database(e.to_string()))?;

    tracing::trace!(
        node_id = %node_id,
        assignment_id = %output.assignment_id,
        output_type = %output_type,
        content_len = output.content.len(),
        "stored task output"
    );

    Ok(())
}

/// Handle task progress events from a node.
async fn handle_task_progress(
    node_id: Uuid,
    progress: &TaskProgressMessage,
    pool: &PgPool,
) -> Result<(), HandleError> {
    use crate::db::task_progress_events::{CreateTaskProgressEvent, TaskProgressEventRepository};

    let event_type = format!("{:?}", progress.event_type).to_lowercase();

    let repo = TaskProgressEventRepository::new(pool);
    repo.create(CreateTaskProgressEvent {
        assignment_id: progress.assignment_id,
        event_type,
        message: progress.message.clone(),
        metadata: progress.metadata.clone(),
        timestamp: progress.timestamp,
    })
    .await
    .map_err(|e| HandleError::Database(e.to_string()))?;

    tracing::debug!(
        node_id = %node_id,
        assignment_id = %progress.assignment_id,
        event_type = ?progress.event_type,
        "stored task progress event"
    );

    Ok(())
}

/// Create a link between a node and a project, persist the link, and broadcast a ProjectSync
/// message to all other nodes in the organization.
///
/// This links a node's local project to a swarm project by:
/// 1. Creating/updating the swarm_project_nodes record
/// 2. Updating node_local_projects.swarm_project_id
/// 3. Broadcasting the link to other nodes
///
/// Note: The `project_id` in LinkProjectMessage is now interpreted as `swarm_project_id`.
///
/// The broadcast excludes the originating node. The project name included in the broadcast is
/// derived from the provided `git_repo_path`. Returns an error if the database operation or the
/// broadcast fails.
async fn handle_link_project(
    node_id: Uuid,
    organization_id: Uuid,
    link: &LinkProjectMessage,
    pool: &PgPool,
    connections: &ConnectionManager,
) -> Result<(), HandleError> {
    use crate::db::node_local_projects::NodeLocalProjectRepository;
    use crate::db::swarm_projects::{LinkSwarmProjectNodeData, SwarmProjectRepository};

    let swarm_project_id = link.project_id; // project_id is now swarm_project_id

    // Verify the swarm project exists and belongs to this organization
    let swarm_project = SwarmProjectRepository::find_by_id(pool, swarm_project_id)
        .await
        .map_err(|e| HandleError::Database(e.to_string()))?
        .ok_or_else(|| {
            HandleError::Database(format!("swarm project {} not found", swarm_project_id))
        })?;

    if swarm_project.organization_id != organization_id {
        return Err(HandleError::Database(format!(
            "swarm project {} does not belong to organization {}",
            swarm_project_id, organization_id
        )));
    }

    // Create/update swarm_project_nodes record
    let swarm_link = SwarmProjectRepository::link_node_pool(
        pool,
        LinkSwarmProjectNodeData {
            swarm_project_id,
            node_id,
            local_project_id: link.local_project_id,
            git_repo_path: link.git_repo_path.clone(),
            os_type: None, // OS type can be added to LinkProjectMessage later if needed
        },
    )
    .await
    .map_err(|e| HandleError::Database(e.to_string()))?;

    // Update node_local_projects.swarm_project_id (upserts if record doesn't exist)
    if let Err(e) = NodeLocalProjectRepository::link_to_swarm_with_upsert(
        pool,
        node_id,
        link.local_project_id,
        swarm_project_id,
        &link.git_repo_path,
        &link.default_branch,
    )
    .await
    {
        tracing::warn!(
            node_id = %node_id,
            swarm_project_id = %swarm_project_id,
            local_project_id = %link.local_project_id,
            error = ?e,
            "failed to update node_local_projects.swarm_project_id (non-fatal)"
        );
    }

    tracing::info!(
        node_id = %node_id,
        swarm_project_id = %swarm_project_id,
        local_project_id = %link.local_project_id,
        git_repo_path = %link.git_repo_path,
        "linked project to swarm"
    );

    // Broadcast the new project link to other nodes
    let service = NodeServiceImpl::new(pool.clone());
    let node = service
        .get_node(node_id)
        .await
        .map_err(|e| HandleError::Database(e.to_string()))?;

    let project_name = extract_project_name(&link.git_repo_path);

    let sync_msg = HiveMessage::ProjectSync(ProjectSyncMessage {
        message_id: Uuid::new_v4(),
        link_id: swarm_link.id,
        project_id: swarm_project_id, // This is now the swarm_project_id
        project_name,
        local_project_id: link.local_project_id,
        git_repo_path: link.git_repo_path.clone(),
        default_branch: link.default_branch.clone(),
        source_node_id: node_id,
        source_node_name: node.name,
        source_node_public_url: node.public_url,
        is_new: true,
    });

    // Only broadcast to nodes that are linked to the same swarm project
    let linked_nodes = SwarmProjectRepository::get_linked_node_ids(pool, swarm_project_id)
        .await
        .map_err(|e| HandleError::Database(e.to_string()))?;

    let target_nodes: Vec<_> = linked_nodes
        .into_iter()
        .filter(|&id| id != node_id)
        .collect();

    if target_nodes.is_empty() {
        tracing::debug!(
            node_id = %node_id,
            swarm_project_id = %swarm_project_id,
            "no other nodes linked to swarm project, skipping broadcast"
        );
        return Ok(());
    }

    let failed = connections.send_to_nodes(&target_nodes, sync_msg).await;

    if !failed.is_empty() {
        tracing::warn!(
            node_id = %node_id,
            swarm_project_id = %swarm_project_id,
            failed_count = failed.len(),
            "failed to broadcast project link to some nodes"
        );
    }

    Ok(())
}

/// Unlinks a node's local project from its swarm project and broadcasts the removal to other nodes in the organization.
///
/// This removes the swarm_project_nodes link, clears the corresponding node_local_projects.swarm_project_id when the local project is known, and — if the link existed — sends a `ProjectSync` message with `is_new = false` containing the project's name, default branch (falls back to `"main"`), and source node information to all other nodes in the same organization.
///
/// # Returns
///
/// `Ok(())` on success, `Err(HandleError)` if a database or send error occurs.
///
/// # Examples
///
/// ```no_run
/// use uuid::Uuid;
/// # async fn doc() {
/// let pool = /* PgPool */ todo!();
/// let connections = /* ConnectionManager */ todo!();
/// let unlink = /* UnlinkProjectMessage */ todo!();
/// handle_unlink_project(Uuid::nil(), Uuid::nil(), &unlink, &pool, &connections).await.unwrap();
/// # }
/// ```
async fn handle_unlink_project(
    node_id: Uuid,
    _organization_id: Uuid,
    unlink: &UnlinkProjectMessage,
    pool: &PgPool,
    connections: &ConnectionManager,
) -> Result<(), HandleError> {
    use crate::db::node_local_projects::NodeLocalProjectRepository;
    use crate::db::swarm_projects::SwarmProjectRepository;

    let swarm_project_id = unlink.project_id; // project_id is now swarm_project_id

    let service = NodeServiceImpl::new(pool.clone());

    // Get node info before unlink for the broadcast
    let node = service
        .get_node(node_id)
        .await
        .map_err(|e| HandleError::Database(e.to_string()))?;

    // Find the swarm_project_nodes link to get the local_project_id
    let link_info = SwarmProjectRepository::find_node_link(pool, swarm_project_id, node_id)
        .await
        .map_err(|e| HandleError::Database(e.to_string()))?;

    // Remove the swarm_project_nodes link
    if let Err(e) = SwarmProjectRepository::unlink_node_pool(pool, swarm_project_id, node_id).await {
        tracing::warn!(
            node_id = %node_id,
            swarm_project_id = %swarm_project_id,
            error = ?e,
            "failed to unlink swarm_project_nodes (may not exist)"
        );
    }

    // Clear node_local_projects.swarm_project_id if we know the local_project_id
    if let Some(ref link) = link_info
        && let Err(e) =
            NodeLocalProjectRepository::unlink_from_swarm_pool(pool, node_id, link.local_project_id)
                .await
    {
        tracing::warn!(
            node_id = %node_id,
            local_project_id = %link.local_project_id,
            error = ?e,
            "failed to clear node_local_projects.swarm_project_id (non-fatal)"
        );
    }

    tracing::info!(
        node_id = %node_id,
        swarm_project_id = %swarm_project_id,
        "unlinked project from swarm"
    );

    // Broadcast the unlink to other nodes (only if we found the link info)
    if let Some(link) = link_info {
        let project_name = extract_project_name(&link.git_repo_path);

        // Look up default_branch from node_local_projects (or use "main" as fallback)
        let default_branch = NodeLocalProjectRepository::find_by_node_and_project(
            pool,
            node_id,
            link.local_project_id,
        )
        .await
        .ok()
        .flatten()
        .map(|nlp| nlp.default_branch)
        .unwrap_or_else(|| "main".to_string());

        let sync_msg = HiveMessage::ProjectSync(ProjectSyncMessage {
            message_id: Uuid::new_v4(),
            link_id: link.id,
            project_id: swarm_project_id, // This is now the swarm_project_id
            project_name,
            local_project_id: link.local_project_id,
            git_repo_path: link.git_repo_path,
            default_branch,
            source_node_id: node_id,
            source_node_name: node.name,
            source_node_public_url: node.public_url,
            is_new: false, // false indicates removal
        });

        // Only broadcast to nodes that are still linked to this swarm project
        // (the unlinking node was already removed at this point)
        let linked_nodes = SwarmProjectRepository::get_linked_node_ids(pool, swarm_project_id)
            .await
            .unwrap_or_default();

        if linked_nodes.is_empty() {
            tracing::debug!(
                node_id = %node_id,
                swarm_project_id = %swarm_project_id,
                "no other nodes linked to swarm project, skipping unlink broadcast"
            );
        } else {
            let failed = connections.send_to_nodes(&linked_nodes, sync_msg).await;

            if !failed.is_empty() {
                tracing::warn!(
                    node_id = %node_id,
                    swarm_project_id = %swarm_project_id,
                    failed_count = failed.len(),
                    "failed to broadcast project unlink to some nodes"
                );
            }
        }
    }

    Ok(())
}

/// Deregisters a node by removing its database records and notifying the organization.
///
/// Deletes the node and its related data, then broadcasts a `NodeRemoved` message to other nodes in the same organization. Any failures to notify peers are logged but do not prevent completion of the deregistration.
///
/// # Examples
///
/// ```
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// # // placeholders for required values
/// # let pool = todo!("PgPool instance");
/// # let connections = todo!("ConnectionManager instance");
/// use uuid::Uuid;
/// let node_id = Uuid::new_v4();
/// let org_id = Uuid::new_v4();
/// let dereg_msg = DeregisterMessage {
///     message_id: Uuid::new_v4(),
///     reason: Some("decommission".into()),
/// };
/// // Call the async function (this doc example is illustrative and uses placeholders)
/// let _ = handle_deregister(node_id, org_id, &dereg_msg, &pool, &connections).await?;
/// # Ok(())
/// # }
/// ```
///
/// # Returns
///
/// `Ok(())` on success, or `Err(HandleError::Database(_))` if deleting the node from the database fails.
async fn handle_deregister(
    node_id: Uuid,
    organization_id: Uuid,
    deregister: &DeregisterMessage,
    pool: &PgPool,
    connections: &ConnectionManager,
) -> Result<(), HandleError> {
    tracing::info!(
        node_id = %node_id,
        message_id = %deregister.message_id,
        reason = ?deregister.reason,
        "node requesting deregistration"
    );

    let service = NodeServiceImpl::new(pool.clone());

    // Delete the node (cascades all related data: swarm_project_nodes, task_assignments)
    service
        .delete_node(node_id)
        .await
        .map_err(|e| HandleError::Database(e.to_string()))?;

    // Broadcast node removal to all other nodes in the organization
    let removal_msg = HiveMessage::NodeRemoved(NodeRemovedMessage {
        node_id,
        reason: deregister
            .reason
            .clone()
            .unwrap_or_else(|| "Node deregistered".to_string()),
    });

    let failed = connections
        .broadcast_to_org(organization_id, removal_msg)
        .await;
    if !failed.is_empty() {
        tracing::warn!(
            node_id = %node_id,
            failed_count = failed.len(),
            "failed to notify some nodes of deregistration"
        );
    }

    tracing::info!(
        node_id = %node_id,
        "node deregistered successfully"
    );

    Ok(())
}

/// Error when handling a node message.
#[derive(Debug, thiserror::Error)]
enum HandleError {
    #[error("database error: {0}")]
    Database(String),
    #[error("failed to send message")]
    Send,
}

/// Sanitize a string by removing null bytes (0x00).
///
/// PostgreSQL does not allow null bytes in text fields, so this function
/// strips them to prevent "invalid byte sequence for encoding UTF8: 0x00" errors.
fn sanitize_string(s: &str) -> String {
    s.replace('\0', "")
}

/// Sanitize an optional string by removing null bytes.
fn sanitize_option_string(s: Option<String>) -> Option<String> {
    s.map(|v| sanitize_string(&v))
}

/// Send a message to the WebSocket.
async fn send_message(
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    msg: &HiveMessage,
) -> Result<(), ()> {
    match serde_json::to_string(msg) {
        Ok(json) => sender
            .send(Message::Text(json.into()))
            .await
            .map_err(|error| {
                tracing::debug!(?error, "failed to send websocket message");
            }),
        Err(error) => {
            tracing::error!(?error, "failed to serialize message");
            Err(())
        }
    }
}

/// Upserts a node task attempt and, when the attempt has no `assignment_id`, tries to create a synthetic assignment for locally-started tasks.
///
/// If `attempt.assignment_id` is provided, that value is used. If it is `None`, the function looks up the shared task's `swarm_project_id`, finds the node's `swarm_project_nodes` link for that project, and attempts to create or find a synthetic assignment tied to that link. Failures to create or find a synthetic assignment are logged and do not prevent the attempt from being upserted; the attempt will be stored without an `assignment_id` in that case. The upsert writes the attempt data into the `node_task_attempts` table.
///
/// # Examples
///
/// ```
/// # async fn example(pool: &sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
/// use uuid::Uuid;
/// use chrono::Utc;
/// use crate::nodes::ws::messages::AttemptSyncMessage;
///
/// let node_id = Uuid::new_v4();
///
/// let attempt = AttemptSyncMessage {
///     attempt_id: Uuid::new_v4(),
///     shared_task_id: Uuid::new_v4(),
///     assignment_id: None,
///     executor: None,
///     executor_variant: None,
///     branch: None,
///     target_branch: None,
///     container_ref: None,
///     worktree_deleted: false,
///     setup_completed_at: None,
///     created_at: Utc::now(),
///     updated_at: Utc::now(),
/// };
///
/// crate::nodes::ws::session::handle_attempt_sync(node_id, &attempt, pool).await?;
/// # Ok(())
/// # }
/// ```
async fn handle_attempt_sync(
    node_id: Uuid,
    attempt: &AttemptSyncMessage,
    pool: &PgPool,
) -> Result<(), HandleError> {
    use crate::db::node_task_attempts::{NodeTaskAttemptRepository, UpsertNodeTaskAttempt};
    use crate::db::swarm_projects::SwarmProjectRepository;
    use crate::db::task_assignments::TaskAssignmentRepository;
    use crate::db::tasks::SharedTaskRepository;

    // Determine assignment_id: use provided one or create a synthetic one
    let assignment_id = match attempt.assignment_id {
        Some(id) => Some(id),
        None => {
            // For locally-started tasks, we need to create a synthetic assignment
            // First, find the swarm_project_id from the shared_task
            let shared_task_repo = SharedTaskRepository::new(pool);
            let shared_task = shared_task_repo
                .find_by_id(attempt.shared_task_id)
                .await
                .map_err(|e| HandleError::Database(e.to_string()))?;

            if let Some(task) = shared_task {
                // Find the swarm_project_nodes link for this project and node
                // swarm_project_id is the source of truth
                if let Some(swarm_project_id) = task.swarm_project_id {
                    let node_link = SwarmProjectRepository::find_node_link(
                        pool,
                        swarm_project_id,
                        node_id,
                    )
                    .await
                    .map_err(|e| HandleError::Database(e.to_string()))?;

                    if let Some(link) = node_link {
                        // Create or find a synthetic assignment
                        // Use the swarm_project_nodes link ID as the node_project_id
                        let assignment_repo = TaskAssignmentRepository::new(pool);
                        match assignment_repo
                            .create_or_find_synthetic(attempt.shared_task_id, node_id, link.id)
                            .await
                        {
                            Ok(assignment) => {
                                tracing::info!(
                                    node_id = %node_id,
                                    attempt_id = %attempt.attempt_id,
                                    assignment_id = %assignment.id,
                                    "created synthetic assignment for locally-started task"
                                );
                                Some(assignment.id)
                            }
                            Err(e) => {
                                tracing::warn!(
                                    node_id = %node_id,
                                    attempt_id = %attempt.attempt_id,
                                    error = %e,
                                    "failed to create synthetic assignment, proceeding without"
                                );
                                None
                            }
                        }
                    } else {
                        tracing::debug!(
                            node_id = %node_id,
                            swarm_project_id = %swarm_project_id,
                            "no swarm_project_nodes link found for synthetic assignment"
                        );
                        None
                    }
                } else {
                    tracing::debug!(
                        node_id = %node_id,
                        shared_task_id = %attempt.shared_task_id,
                        "task has no swarm_project_id, skipping synthetic assignment"
                    );
                    None
                }
            } else {
                tracing::debug!(
                    shared_task_id = %attempt.shared_task_id,
                    "shared task not found for synthetic assignment"
                );
                None
            }
        }
    };

    let repo = NodeTaskAttemptRepository::new(pool);
    repo.upsert(&UpsertNodeTaskAttempt {
        id: attempt.attempt_id,
        assignment_id,
        shared_task_id: attempt.shared_task_id,
        node_id,
        executor: attempt.executor.clone(),
        executor_variant: attempt.executor_variant.clone(),
        branch: attempt.branch.clone(),
        target_branch: attempt.target_branch.clone(),
        container_ref: attempt.container_ref.clone(),
        worktree_deleted: attempt.worktree_deleted,
        setup_completed_at: attempt.setup_completed_at,
        created_at: attempt.created_at,
        updated_at: attempt.updated_at,
    })
    .await
    .map_err(|e| HandleError::Database(e.to_string()))?;

    tracing::debug!(
        node_id = %node_id,
        attempt_id = %attempt.attempt_id,
        shared_task_id = %attempt.shared_task_id,
        assignment_id = ?assignment_id,
        "synced task attempt from node"
    );

    Ok(())
}

/// Handle an execution sync message from a node.
///
/// Upserts the execution process into node_execution_processes.
/// If the referenced attempt doesn't exist yet (race condition during sync),
/// we log a warning and skip - the client will retry on the next sync cycle.
async fn handle_execution_sync(
    node_id: Uuid,
    execution: &ExecutionSyncMessage,
    pool: &PgPool,
) -> Result<(), HandleError> {
    use crate::db::node_execution_processes::{
        NodeExecutionProcessError, NodeExecutionProcessRepository, UpsertNodeExecutionProcess,
    };
    use crate::db::node_task_attempts::NodeTaskAttemptRepository;

    // Check if the parent attempt exists first to avoid FK constraint errors
    // This can happen when ExecutionSync arrives before AttemptSync is processed
    let attempt_repo = NodeTaskAttemptRepository::new(pool);
    let attempt_exists = attempt_repo
        .find_by_id(execution.attempt_id)
        .await
        .map_err(|e| HandleError::Database(e.to_string()))?
        .is_some();

    if !attempt_exists {
        tracing::warn!(
            node_id = %node_id,
            execution_id = %execution.execution_id,
            attempt_id = %execution.attempt_id,
            "execution sync skipped: parent attempt not yet synced (will retry)"
        );
        // Return Ok - the client should retry on next sync cycle
        // This is a race condition, not a real error
        return Ok(());
    }

    let repo = NodeExecutionProcessRepository::new(pool);
    match repo
        .upsert(&UpsertNodeExecutionProcess {
            id: execution.execution_id,
            attempt_id: execution.attempt_id,
            node_id,
            run_reason: execution.run_reason.clone(),
            executor_action: execution.executor_action.clone(),
            before_head_commit: execution.before_head_commit.clone(),
            after_head_commit: execution.after_head_commit.clone(),
            status: execution.status.clone(),
            exit_code: execution.exit_code,
            dropped: execution.dropped,
            pid: execution.pid,
            started_at: execution.started_at,
            completed_at: execution.completed_at,
            created_at: execution.created_at,
        })
        .await
    {
        Ok(_) => {
            tracing::debug!(
                node_id = %node_id,
                execution_id = %execution.execution_id,
                attempt_id = %execution.attempt_id,
                status = %execution.status,
                "synced execution process from node"
            );
            Ok(())
        }
        Err(NodeExecutionProcessError::AttemptNotFound(attempt_id)) => {
            // The attempt doesn't exist on the hive (perhaps deleted or not yet synced).
            // Log a warning but don't fail - this is a transient sync issue.
            tracing::warn!(
                node_id = %node_id,
                execution_id = %execution.execution_id,
                attempt_id = %attempt_id,
                "skipping execution sync: attempt not found on hive"
            );
            Ok(())
        }
        Err(e) => Err(HandleError::Database(e.to_string())),
    }
}

/// Handle a logs batch message from a node.
///
/// Stores the log entries in node_task_output_logs with execution_process_id.
/// If the assignment doesn't exist yet (race condition with AttemptSync), creates
/// a synthetic assignment using the shared_task_id.
async fn handle_logs_batch(
    node_id: Uuid,
    logs: &LogsBatchMessage,
    pool: &PgPool,
) -> Result<(), HandleError> {
    use crate::db::swarm_projects::SwarmProjectRepository;
    use crate::db::task_assignments::TaskAssignmentRepository;
    use crate::db::task_output_logs::{CreateTaskOutputLog, TaskOutputLogRepository};
    use crate::db::tasks::SharedTaskRepository;

    // First, ensure the assignment exists. If not, try to create a synthetic one.
    let assignment_repo = TaskAssignmentRepository::new(pool);
    let assignment_id = match assignment_repo.find_by_id(logs.assignment_id).await {
        Ok(Some(_)) => {
            // Assignment exists, use it
            logs.assignment_id
        }
        Ok(None) => {
            // Assignment doesn't exist - try to create synthetic if we have shared_task_id
            if let Some(shared_task_id) = logs.shared_task_id {
                // Look up the task to find swarm_project_id
                let shared_task_repo = SharedTaskRepository::new(pool);
                let shared_task = shared_task_repo
                    .find_by_id(shared_task_id)
                    .await
                    .map_err(|e| HandleError::Database(e.to_string()))?;

                if let Some(task) = shared_task {
                    if let Some(swarm_project_id) = task.swarm_project_id {
                        // Find the node's link to this project
                        let node_link = SwarmProjectRepository::find_node_link(
                            pool,
                            swarm_project_id,
                            node_id,
                        )
                        .await
                        .map_err(|e| HandleError::Database(e.to_string()))?;

                        if let Some(link) = node_link {
                            // Create synthetic assignment
                            match assignment_repo
                                .create_or_find_synthetic(shared_task_id, node_id, link.id)
                                .await
                            {
                                Ok(assignment) => {
                                    tracing::info!(
                                        node_id = %node_id,
                                        original_assignment_id = %logs.assignment_id,
                                        new_assignment_id = %assignment.id,
                                        shared_task_id = %shared_task_id,
                                        "created synthetic assignment for logs batch"
                                    );
                                    assignment.id
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        node_id = %node_id,
                                        assignment_id = %logs.assignment_id,
                                        error = %e,
                                        "failed to create synthetic assignment for logs"
                                    );
                                    return Err(HandleError::Database(format!(
                                        "failed to create synthetic assignment: {}",
                                        e
                                    )));
                                }
                            }
                        } else {
                            tracing::warn!(
                                node_id = %node_id,
                                swarm_project_id = %swarm_project_id,
                                "no node link found for logs batch - cannot create assignment"
                            );
                            return Err(HandleError::Database(
                                "no node link found for project".to_string(),
                            ));
                        }
                    } else {
                        tracing::warn!(
                            node_id = %node_id,
                            shared_task_id = %shared_task_id,
                            "task has no swarm_project_id - cannot create assignment"
                        );
                        return Err(HandleError::Database(
                            "task has no swarm_project_id".to_string(),
                        ));
                    }
                } else {
                    tracing::warn!(
                        node_id = %node_id,
                        shared_task_id = %shared_task_id,
                        "shared task not found - cannot create assignment"
                    );
                    return Err(HandleError::Database("shared task not found".to_string()));
                }
            } else {
                // No shared_task_id provided, cannot create synthetic assignment
                tracing::warn!(
                    node_id = %node_id,
                    assignment_id = %logs.assignment_id,
                    "assignment not found and no shared_task_id provided"
                );
                return Err(HandleError::Database(format!(
                    "assignment {} not found and no shared_task_id provided",
                    logs.assignment_id
                )));
            }
        }
        Err(e) => {
            return Err(HandleError::Database(e.to_string()));
        }
    };

    // Now insert the logs with the valid assignment_id
    let log_repo = TaskOutputLogRepository::new(pool);

    for entry in &logs.entries {
        let output_type = match entry.output_type {
            super::message::TaskOutputType::Stdout => "stdout",
            super::message::TaskOutputType::Stderr => "stderr",
            super::message::TaskOutputType::System => "system",
        };

        // Create log with optional execution_process_id
        log_repo
            .create_with_execution_process(
                CreateTaskOutputLog {
                    assignment_id,
                    output_type: output_type.to_string(),
                    content: entry.content.clone(),
                    timestamp: entry.timestamp,
                },
                logs.execution_process_id,
            )
            .await
            .map_err(|e| HandleError::Database(e.to_string()))?;
    }

    tracing::trace!(
        node_id = %node_id,
        assignment_id = %assignment_id,
        execution_process_id = ?logs.execution_process_id,
        entry_count = logs.entries.len(),
        "stored logs batch from node"
    );

    Ok(())
}

// NOTE: handle_label_sync has been removed.
// Labels are now managed centrally on the hive and synced DOWN to nodes.
// NodeMessage::LabelSync is ignored (see route_message above).

/// Handle a task sync message from a node.
///
/// The new flow looks up the swarm project via node_local_projects → swarm_project_nodes,
/// using the node's local_project_id to find the linked swarm_project_id.
///
/// Race condition handling:
/// - If node_local_projects doesn't exist → ProjectsSync hasn't arrived, send RETRY
/// - If node_local_projects exists but swarm_project_id is NULL → not linked, send error
/// - If fully linked → proceed with sync
async fn handle_task_sync(
    node_id: Uuid,
    organization_id: Uuid,
    task_sync: &TaskSyncMessage,
    pool: &PgPool,
    ws_sender: &mut futures::stream::SplitSink<WebSocket, Message>,
) -> Result<(), HandleError> {
    use crate::db::node_local_projects::NodeLocalProjectRepository;
    use crate::db::tasks::{SharedTaskRepository, TaskStatus, UpsertTaskFromNodeData};

    let status = match task_sync.status.as_str() {
        "todo" => TaskStatus::Todo,
        "in_progress" | "in-progress" => TaskStatus::InProgress,
        "in_review" | "in-review" => TaskStatus::InReview,
        "done" => TaskStatus::Done,
        "cancelled" => TaskStatus::Cancelled,
        _ => TaskStatus::Todo,
    };

    // First check if the node_local_projects record exists
    // This distinguishes between "ProjectsSync hasn't arrived" and "project not linked"
    let local_project = NodeLocalProjectRepository::find_by_node_and_project(
        pool,
        node_id,
        task_sync.local_project_id,
    )
    .await
    .map_err(|e| HandleError::Database(e.to_string()))?;

    let local_project = match local_project {
        Some(p) => p,
        None => {
            // Race condition: ProjectsSync hasn't arrived yet
            // Tell the node to retry after its ProjectsSync completes
            tracing::debug!(
                node_id = %node_id,
                local_project_id = %task_sync.local_project_id,
                local_task_id = %task_sync.local_task_id,
                "task sync: project not found in node_local_projects (ProjectsSync race, retry)"
            );
            let r = TaskSyncResponseMessage {
                local_task_id: task_sync.local_task_id,
                shared_task_id: Uuid::nil(),
                success: false,
                error: Some("RETRY: Project not synced yet, please retry after ProjectsSync completes".to_string()),
            };
            let _ = send_message(ws_sender, &HiveMessage::TaskSyncResponse(r)).await;
            return Ok(());
        }
    };

    // Check if the project is linked to a swarm project
    let swarm_project_id = match local_project.swarm_project_id {
        Some(id) => id,
        None => {
            // Project exists in node_local_projects but isn't linked to swarm
            // This is intentional - the project needs to be linked via UI first
            tracing::debug!(
                node_id = %node_id,
                local_project_id = %task_sync.local_project_id,
                local_task_id = %task_sync.local_task_id,
                "task sync: project not linked to swarm (link via UI first)"
            );
            let r = TaskSyncResponseMessage {
                local_task_id: task_sync.local_task_id,
                shared_task_id: Uuid::nil(),
                success: false,
                error: Some("Project not linked to swarm - link it via the swarm settings UI first".to_string()),
            };
            let _ = send_message(ws_sender, &HiveMessage::TaskSyncResponse(r)).await;
            return Ok(());
        }
    };

    // Verify the swarm project belongs to this organization and node has a link
    let swarm_link: Option<SwarmProjectLink> = sqlx::query_as(
        r#"
        SELECT
            sp.organization_id
        FROM swarm_project_nodes spn
        JOIN swarm_projects sp ON spn.swarm_project_id = sp.id
        WHERE spn.node_id = $1
          AND spn.local_project_id = $2
          AND spn.swarm_project_id = $3
          AND sp.organization_id = $4
        "#,
    )
    .bind(node_id)
    .bind(task_sync.local_project_id)
    .bind(swarm_project_id)
    .bind(organization_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| HandleError::Database(e.to_string()))?;

    let org_id = match swarm_link {
        Some(link) => link.organization_id,
        None => {
            tracing::warn!(
                node_id = %node_id,
                local_project_id = %task_sync.local_project_id,
                swarm_project_id = %swarm_project_id,
                "task sync failed: swarm_project_nodes link missing or org mismatch"
            );
            let r = TaskSyncResponseMessage {
                local_task_id: task_sync.local_task_id,
                shared_task_id: Uuid::nil(),
                success: false,
                error: Some("Swarm project link invalid - re-link the project via UI".to_string()),
            };
            let _ = send_message(ws_sender, &HiveMessage::TaskSyncResponse(r)).await;
            return Ok(());
        }
    };

    // Sanitize string fields to remove null bytes (PostgreSQL doesn't allow them)
    let sanitized_title = sanitize_string(&task_sync.title);
    let sanitized_description = sanitize_option_string(task_sync.description.clone());

    let repo = SharedTaskRepository::new(pool);
    match repo
        .upsert_from_node(UpsertTaskFromNodeData {
            swarm_project_id,
            project_id: swarm_project_id, // Use swarm_project_id for both (backwards compat)
            organization_id: org_id,
            origin_node_id: node_id,
            local_task_id: task_sync.local_task_id,
            title: sanitized_title,
            description: sanitized_description,
            status,
            version: task_sync.version,
            owner_node_id: task_sync.owner_node_id,
            owner_name: task_sync.owner_name.clone(),
        })
        .await
    {
        Ok((task, was_created)) => {
            tracing::info!(
                node_id = %node_id,
                shared_task_id = %task.id,
                swarm_project_id = %swarm_project_id,
                was_created = was_created,
                "synced task from node"
            );
            let r = TaskSyncResponseMessage {
                local_task_id: task_sync.local_task_id,
                shared_task_id: task.id,
                success: true,
                error: None,
            };
            let _ = send_message(ws_sender, &HiveMessage::TaskSyncResponse(r)).await;
        }
        Err(e) => {
            tracing::error!(node_id = %node_id, error = ?e, "failed to sync task");
            let r = TaskSyncResponseMessage {
                local_task_id: task_sync.local_task_id,
                shared_task_id: Uuid::nil(),
                success: false,
                error: Some(e.to_string()),
            };
            let _ = send_message(ws_sender, &HiveMessage::TaskSyncResponse(r)).await;
        }
    }
    Ok(())
}

/// Helper struct for swarm project link lookup
#[derive(sqlx::FromRow)]
struct SwarmProjectLink {
    organization_id: Uuid,
}

/// Handle a projects sync message from a node.
///
/// This upserts all local projects from the node into the `node_local_projects`
/// table, enabling the swarm settings UI to show all projects for linking.
async fn handle_projects_sync(
    node_id: Uuid,
    projects: &ProjectsSyncMessage,
    pool: &PgPool,
) -> Result<(), HandleError> {
    use crate::db::node_local_projects::{NodeLocalProjectRepository, UpsertLocalProjectData};

    let projects_data: Vec<UpsertLocalProjectData> = projects
        .projects
        .iter()
        .map(|p| UpsertLocalProjectData {
            node_id,
            local_project_id: p.local_project_id,
            name: p.name.clone(),
            git_repo_path: p.git_repo_path.clone(),
            default_branch: p.default_branch.clone(),
        })
        .collect();

    let count = projects_data.len();
    match NodeLocalProjectRepository::bulk_upsert(pool, node_id, projects_data).await {
        Ok(upserted) => {
            tracing::info!(
                node_id = %node_id,
                total = count,
                upserted = upserted,
                "synced local projects from node"
            );
        }
        Err(e) => {
            tracing::error!(
                node_id = %node_id,
                error = ?e,
                "failed to sync local projects"
            );
        }
    }

    Ok(())
}

/// Broadcast a node's owned projects to other nodes linked to the same swarm projects.
///
/// This is called when a node connects to notify other nodes about the newly
/// connected node's available projects. Only projects owned by this node
/// (is_owned == true) are broadcast, and only to nodes linked to the same swarm project.
async fn broadcast_node_projects(
    node_id: Uuid,
    _organization_id: Uuid,
    node_name: &str,
    node_public_url: Option<&str>,
    linked_projects: &[LinkedProjectInfo],
    pool: &PgPool,
    connections: &ConnectionManager,
) {
    use crate::db::swarm_projects::SwarmProjectRepository;

    // Only broadcast projects owned by this node
    let owned_projects: Vec<_> = linked_projects.iter().filter(|p| p.is_owned).collect();

    if owned_projects.is_empty() {
        return;
    }

    let mut broadcast_count = 0;

    for project_info in &owned_projects {
        // Get nodes linked to this swarm project (project_id is swarm_project_id)
        let linked_nodes = match SwarmProjectRepository::get_linked_node_ids(pool, project_info.project_id).await {
            Ok(nodes) => nodes,
            Err(e) => {
                tracing::warn!(
                    node_id = %node_id,
                    project_id = %project_info.project_id,
                    error = ?e,
                    "failed to get linked nodes for project, skipping broadcast"
                );
                continue;
            }
        };

        // Filter out the current node
        let target_nodes: Vec<_> = linked_nodes
            .into_iter()
            .filter(|&id| id != node_id)
            .collect();

        if target_nodes.is_empty() {
            tracing::debug!(
                node_id = %node_id,
                project_id = %project_info.project_id,
                "no other nodes linked to swarm project, skipping broadcast"
            );
            continue;
        }

        let sync_msg = HiveMessage::ProjectSync(ProjectSyncMessage {
            message_id: Uuid::new_v4(),
            link_id: project_info.link_id,
            project_id: project_info.project_id,
            project_name: project_info.project_name.clone(),
            local_project_id: project_info.local_project_id,
            git_repo_path: project_info.git_repo_path.clone(),
            default_branch: project_info.default_branch.clone(),
            source_node_id: node_id,
            source_node_name: node_name.to_string(),
            source_node_public_url: node_public_url.map(String::from),
            is_new: true,
        });

        let failed = connections.send_to_nodes(&target_nodes, sync_msg).await;
        broadcast_count += 1;

        if !failed.is_empty() {
            tracing::warn!(
                node_id = %node_id,
                project_id = %project_info.project_id,
                failed_count = failed.len(),
                "failed to broadcast project to some nodes"
            );
        }
    }

    tracing::info!(
        node_id = %node_id,
        owned_project_count = owned_projects.len(),
        broadcast_count = broadcast_count,
        "broadcast node's owned projects to linked nodes"
    );
}

/// Handle a backfill response from a node.
///
/// When a node responds to a backfill request, it sends the actual data via
/// normal AttemptSync/ExecutionSync/LogsBatch messages. This response just
/// confirms completion and allows us to mark attempts as complete.
async fn handle_backfill_response(
    node_id: Uuid,
    response: &BackfillResponseMessage,
    pool: &PgPool,
    tracker: &BackfillRequestTracker,
) -> Result<(), HandleError> {
    tracing::info!(
        node_id = %node_id,
        request_id = %response.request_id,
        success = response.success,
        entities_sent = response.entities_sent,
        error = ?response.error,
        "received backfill response from node"
    );

    // Look up the attempt IDs that were tracked for this request
    let attempt_ids = tracker.complete(response.request_id).await;

    let repo = NodeTaskAttemptRepository::new(pool);

    if response.success {
        // Mark each tracked attempt as complete
        match attempt_ids {
            Some(ids) if !ids.is_empty() => {
                tracing::debug!(
                    node_id = %node_id,
                    request_id = %response.request_id,
                    attempt_count = ids.len(),
                    "marking tracked attempts as complete"
                );

                for attempt_id in ids {
                    if let Err(e) = repo.mark_complete(attempt_id).await {
                        tracing::error!(
                            node_id = %node_id,
                            attempt_id = %attempt_id,
                            error = %e,
                            "failed to mark attempt as complete"
                        );
                    }
                }
            }
            _ => {
                // No tracked mapping found - this could happen if:
                // - Request was made before tracker was integrated
                // - Tracker was cleared due to node disconnect/reconnect
                // - Stale cleanup removed the mapping
                tracing::warn!(
                    node_id = %node_id,
                    request_id = %response.request_id,
                    "no tracked attempts found for backfill response"
                );
            }
        }
    } else {
        // Log the error - attempts need to be reset to partial state
        tracing::warn!(
            node_id = %node_id,
            request_id = %response.request_id,
            error = ?response.error,
            "backfill request failed"
        );

        // Reset tracked attempts to partial state so they can be retried
        match attempt_ids {
            Some(ids) if !ids.is_empty() => {
                tracing::debug!(
                    node_id = %node_id,
                    request_id = %response.request_id,
                    attempt_count = ids.len(),
                    "resetting tracked attempts to partial"
                );

                for attempt_id in ids {
                    if let Err(e) = repo.reset_attempt_to_partial(attempt_id).await {
                        tracing::error!(
                            node_id = %node_id,
                            attempt_id = %attempt_id,
                            error = %e,
                            "failed to reset attempt to partial"
                        );
                    }
                }
            }
            _ => {
                // No tracked mapping found - fall back to resetting all failed backfills for node
                tracing::warn!(
                    node_id = %node_id,
                    request_id = %response.request_id,
                    "no tracked attempts found, falling back to reset_failed_backfill"
                );

                if let Err(e) = repo.reset_failed_backfill(node_id).await {
                    tracing::error!(
                        node_id = %node_id,
                        error = %e,
                        "failed to reset failed backfill state"
                    );
                }
            }
        }
    }

    Ok(())
}