//! Task dispatcher for streaming task assignments to nodes.
//!
//! This module handles the assignment and dispatch of tasks to connected nodes.

use sqlx::PgPool;
use uuid::Uuid;

use super::{
    connection::ConnectionManager,
    message::{HiveMessage, TaskAssignMessage, TaskDetails},
};
use crate::db::swarm_projects::{SwarmProjectNodeForDispatch, SwarmProjectRepository};
use crate::nodes::service::{NodeError, NodeServiceImpl};

/// Dispatcher for sending tasks to nodes.
#[derive(Clone)]
pub struct TaskDispatcher {
    pool: PgPool,
    connections: ConnectionManager,
}

impl TaskDispatcher {
    /// Create a new task dispatcher.
    pub fn new(pool: PgPool, connections: ConnectionManager) -> Self {
        Self { pool, connections }
    }

    /// Assigns a task to the first connected node linked to a swarm project.
    ///
    /// On success returns an AssignResult containing the created assignment ID and the node ID; on failure returns a DispatchError.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use uuid::Uuid;
    /// # use crate::nodes::ws::{TaskDispatcher, TaskDetails};
    /// # async fn example(dispatcher: &TaskDispatcher) -> Result<(), Box<dyn std::error::Error>> {
    /// let task_id = Uuid::new_v4();
    /// let swarm_project_id = Uuid::new_v4();
    /// let details = TaskDetails::default();
    /// let result = dispatcher.assign_task(task_id, swarm_project_id, details).await?;
    /// println!("assignment {} sent to node {}", result.assignment_id, result.node_id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn assign_task(
        &self,
        task_id: Uuid,
        swarm_project_id: Uuid,
        task_details: TaskDetails,
    ) -> Result<AssignResult, DispatchError> {
        let node = self.find_connected_node(swarm_project_id).await?;
        let result = self.assign_task_to_node(task_id, &node, task_details).await?;

        tracing::info!(
            task_id = %task_id,
            node_id = %result.node_id,
            assignment_id = %result.assignment_id,
            swarm_project_id = %swarm_project_id,
            "task assigned to node"
        );

        Ok(result)
    }

    /// Assigns a task to a previously selected node and dispatches the assignment message.
    ///
    /// Use this after `find_connected_node` to ensure the same node chosen for selection is used for dispatch.
    /// This verifies the node is still connected, records the assignment in storage, and sends a task-assign
    /// message to the node.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use uuid::Uuid;
    /// # async fn example(dispatcher: &crate::nodes::ws::TaskDispatcher, node: &crate::db::swarm_projects::SwarmProjectNodeForDispatch) {
    /// let task_id = Uuid::new_v4();
    /// let details = crate::nodes::ws::TaskDetails { /* fields omitted */ };
    /// let result = dispatcher.assign_task_to_node(task_id, node, details).await;
    /// # }
    /// ```
    pub async fn assign_task_to_node(
        &self,
        task_id: Uuid,
        node: &SwarmProjectNodeForDispatch,
        task_details: TaskDetails,
    ) -> Result<AssignResult, DispatchError> {
        // Verify node is still connected
        if !self.connections.is_connected(node.node_id).await {
            return Err(DispatchError::NodeNotConnected);
        }

        let service = NodeServiceImpl::new(self.pool.clone());

        // Create the assignment (use swarm_project_nodes link_id as node_project_id)
        let assignment = service
            .assign_task(task_id, node.node_id, node.link_id)
            .await?;

        // Build the assign message
        let message = HiveMessage::TaskAssign(TaskAssignMessage {
            message_id: Uuid::new_v4(),
            assignment_id: assignment.id,
            task_id,
            node_project_id: node.link_id,
            local_project_id: node.local_project_id,
            task: task_details,
        });

        // Send to the node
        self.connections
            .send_to_node(node.node_id, message)
            .await
            .map_err(|e| DispatchError::SendFailed(e.to_string()))?;

        tracing::info!(
            task_id = %task_id,
            node_id = %node.node_id,
            assignment_id = %assignment.id,
            "task assigned to pre-selected node"
        );

        Ok(AssignResult {
            assignment_id: assignment.id,
            node_id: node.node_id,
        })
    }

    /// Finds the first connected node linked to the given swarm project.
    ///
    /// Looks up nodes associated with `swarm_project_id` and returns the first one that is currently connected.
    ///
    /// # Returns
    ///
    /// A `SwarmProjectNodeForDispatch` for a connected node, or a `DispatchError` if no nodes exist for the project,
    /// none are currently connected, or a node-service operation fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use uuid::Uuid;
    /// # async fn example(dispatcher: &crate::nodes::ws::dispatcher::TaskDispatcher) -> Result<(), ()> {
    /// let swarm_project_id = Uuid::new_v4();
    /// let node = dispatcher.find_connected_node(swarm_project_id).await?;
    /// // use `node.default_branch` before constructing TaskDetails
    /// # Ok(())
    /// # }
    /// ```
    pub async fn find_connected_node(
        &self,
        swarm_project_id: Uuid,
    ) -> Result<SwarmProjectNodeForDispatch, DispatchError> {
        let nodes = SwarmProjectRepository::find_nodes_for_dispatch(&self.pool, swarm_project_id)
            .await
            .map_err(|e| DispatchError::NodeService(e.into()))?;

        if nodes.is_empty() {
            return Err(DispatchError::NoNodeForProject);
        }

        for node in nodes {
            if self.connections.is_connected(node.node_id).await {
                return Ok(node);
            }
        }

        Err(DispatchError::NodeNotConnected)
    }

    /// Cancel a task on a node.
    ///
    /// Note: This currently only marks the assignment as cancelled in the database.
    /// Sending the cancel message to the node requires knowing which node has the assignment,
    /// which will be implemented when we add assignment tracking.
    pub async fn cancel_task(
        &self,
        assignment_id: Uuid,
        _reason: Option<String>,
    ) -> Result<(), DispatchError> {
        let service = NodeServiceImpl::new(self.pool.clone());

        // Mark the assignment as cancelled in the database
        service
            .update_assignment_status(assignment_id, "cancelled")
            .await?;

        // TODO: Send cancel message to the node once we have assignment->node tracking
        // let message = HiveMessage::TaskCancel(TaskCancelMessage {
        //     message_id: Uuid::new_v4(),
        //     assignment_id,
        //     reason,
        // });

        tracing::info!(assignment_id = %assignment_id, "task cancellation requested");

        Ok(())
    }

    /// Finds an available node in the given organization, creates an assignment for `task_id` using the node's first swarm project link, and dispatches the task to that node.
    ///
    /// On success returns an `AssignResult` containing the created assignment ID and the node ID. On failure returns an appropriate `DispatchError`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use uuid::Uuid;
    /// # use crate::nodes::ws::dispatcher::TaskDispatcher;
    /// # use crate::nodes::ws::dispatcher::AssignResult;
    /// # async fn example(dispatcher: &TaskDispatcher) {
    /// let task_id = Uuid::new_v4();
    /// let organization_id = Uuid::new_v4();
    /// let task_details = /* TaskDetails value */;
    /// let res = dispatcher.dispatch_to_available_node(task_id, organization_id, task_details).await;
    /// assert!(res.is_ok());
    /// # }
    /// ```
    pub async fn dispatch_to_available_node(
        &self,
        task_id: Uuid,
        organization_id: Uuid,
        task_details: TaskDetails,
    ) -> Result<AssignResult, DispatchError> {
        // Find an available node in the organization
        let node_info = self
            .connections
            .find_available_node(organization_id)
            .await
            .ok_or(DispatchError::NoAvailableNode)?;

        let service = NodeServiceImpl::new(self.pool.clone());

        // Get the node's swarm project links to find a suitable one
        let projects = SwarmProjectRepository::list_by_node(&self.pool, node_info.node_id)
            .await
            .map_err(|e| DispatchError::NodeService(e.into()))?;

        if projects.is_empty() {
            return Err(DispatchError::NoProjectOnNode);
        }

        // Use the first project (in production, we'd match based on requirements)
        let project_link = &projects[0];

        // Create the assignment (use swarm_project_nodes link_id as node_project_id)
        let assignment = service
            .assign_task(task_id, node_info.node_id, project_link.id)
            .await?;

        // Build the assign message
        let message = HiveMessage::TaskAssign(TaskAssignMessage {
            message_id: Uuid::new_v4(),
            assignment_id: assignment.id,
            task_id,
            node_project_id: project_link.id,
            local_project_id: project_link.local_project_id,
            task: task_details,
        });

        // Send to the node
        self.connections
            .send_to_node(node_info.node_id, message)
            .await
            .map_err(|e| DispatchError::SendFailed(e.to_string()))?;

        tracing::info!(
            task_id = %task_id,
            node_id = %node_info.node_id,
            assignment_id = %assignment.id,
            "task dispatched to available node"
        );

        Ok(AssignResult {
            assignment_id: assignment.id,
            node_id: node_info.node_id,
        })
    }
}

/// Result of a successful task assignment.
#[derive(Debug, Clone)]
pub struct AssignResult {
    /// The assignment ID
    pub assignment_id: Uuid,
    /// The node that received the task
    pub node_id: Uuid,
}

/// Error when dispatching a task.
#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    #[error("no node is linked to this project")]
    NoNodeForProject,
    #[error("the linked node is not connected")]
    NodeNotConnected,
    #[error("no available node in the organization")]
    NoAvailableNode,
    #[error("the node has no linked projects")]
    NoProjectOnNode,
    #[error("failed to send to node: {0}")]
    SendFailed(String),
    #[error("node service error: {0}")]
    NodeService(#[from] NodeError),
}