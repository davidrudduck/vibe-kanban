//! Unified log service for both local and remote log access.
//!
//! This module provides a service trait and implementations for retrieving
//! paginated log entries regardless of whether the execution is local or remote.

use async_trait::async_trait;
use db::models::log_entry::DbLogEntry;
use sqlx::SqlitePool;
use thiserror::Error;
use utils::unified_log::{Direction, PaginatedLogs};
use uuid::Uuid;

use super::node_proxy_client::{NodeProxyClient, NodeProxyError};

/// Error type for unified log service operations.
#[derive(Debug, Error)]
pub enum LogServiceError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Remote proxy error: {0}")]
    RemoteProxy(#[from] NodeProxyError),

    #[error("Execution not found: {0}")]
    ExecutionNotFound(Uuid),

    #[error("Invalid execution location")]
    InvalidLocation,
}

/// Execution status for determining if logs are still being produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionStatus {
    /// The execution is still running.
    Running,
    /// The execution has completed (successfully or with failure).
    Completed,
    /// The execution status is unknown (e.g., remote execution).
    Unknown,
}

/// Trait for log retrieval services.
///
/// This trait abstracts log retrieval, allowing the same interface to be used
/// for both local (SQLite) and remote (Hive/PostgreSQL) log sources.
#[async_trait]
pub trait LogService: Send + Sync {
    /// Get paginated log entries for an execution.
    ///
    /// # Arguments
    /// * `execution_id` - The execution process ID to fetch logs for
    /// * `cursor` - Optional cursor (entry ID) to start from
    /// * `limit` - Maximum number of entries to return
    /// * `direction` - Forward (oldest first) or Backward (newest first)
    async fn get_logs_paginated(
        &self,
        execution_id: Uuid,
        cursor: Option<i64>,
        limit: i64,
        direction: Direction,
    ) -> Result<PaginatedLogs, LogServiceError>;

    /// Get the execution status.
    ///
    /// Returns whether the execution is still running (producing logs)
    /// or has completed.
    async fn get_execution_status(
        &self,
        execution_id: Uuid,
    ) -> Result<ExecutionStatus, LogServiceError>;
}

/// Local log service using SQLite database.
///
/// This service retrieves logs from the local SQLite database for executions
/// that are running on this node.
#[derive(Debug, Clone)]
pub struct LocalLogService {
    pool: SqlitePool,
}

impl LocalLogService {
    /// Create a new LocalLogService.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LogService for LocalLogService {
    async fn get_logs_paginated(
        &self,
        execution_id: Uuid,
        cursor: Option<i64>,
        limit: i64,
        direction: Direction,
    ) -> Result<PaginatedLogs, LogServiceError> {
        // Use log_entries table for efficient pagination with individual rows
        let paginated = DbLogEntry::find_paginated(
            &self.pool,
            execution_id,
            cursor,
            limit,
            direction,
        )
        .await?;
        Ok(paginated.to_paginated_logs())
    }

    async fn get_execution_status(
        &self,
        execution_id: Uuid,
    ) -> Result<ExecutionStatus, LogServiceError> {
        use db::models::execution_process::ExecutionProcess;

        let process = ExecutionProcess::find_by_id(&self.pool, execution_id).await?;

        match process {
            Some(p) => {
                use db::models::execution_process::ExecutionProcessStatus;
                match p.status {
                    ExecutionProcessStatus::Running => Ok(ExecutionStatus::Running),
                    ExecutionProcessStatus::Completed
                    | ExecutionProcessStatus::Failed
                    | ExecutionProcessStatus::Killed => Ok(ExecutionStatus::Completed),
                }
            }
            None => Err(LogServiceError::ExecutionNotFound(execution_id)),
        }
    }
}

/// Remote log service using node proxy client.
///
/// This service retrieves logs from a remote node via HTTP proxy requests.
/// It's used when viewing executions that are running on other nodes.
#[derive(Debug, Clone)]
pub struct RemoteLogService {
    #[allow(dead_code)] // Will be used in Session 4 when REST endpoint is implemented
    proxy_client: NodeProxyClient,
}

impl RemoteLogService {
    /// Create a new RemoteLogService.
    pub fn new(proxy_client: NodeProxyClient) -> Self {
        Self { proxy_client }
    }
}

/// API response wrapper for remote log requests.
/// Will be used in Session 4 when REST endpoint is implemented.
#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)] // Will be used in Session 4 when REST endpoint is implemented
struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    message: Option<String>,
}

#[async_trait]
impl LogService for RemoteLogService {
    async fn get_logs_paginated(
        &self,
        execution_id: Uuid,
        cursor: Option<i64>,
        limit: i64,
        direction: Direction,
    ) -> Result<PaginatedLogs, LogServiceError> {
        // Build the remote API path with query parameters (for documentation/future use)
        let _direction_str = match direction {
            Direction::Forward => "forward",
            Direction::Backward => "backward",
        };

        let _path = match cursor {
            Some(c) => format!(
                "/logs/{}?limit={}&cursor={}&direction={}",
                execution_id, limit, c, _direction_str
            ),
            None => format!(
                "/logs/{}?limit={}&direction={}",
                execution_id, limit, _direction_str
            ),
        };

        // Note: In a real implementation, we would need the target node ID and URL.
        // For now, this is a placeholder that will be completed in Session 4
        // when we add the REST endpoint and proper routing.
        //
        // The UnifiedLogService will handle determining which node to call.
        Err(LogServiceError::InvalidLocation)
    }

    async fn get_execution_status(
        &self,
        _execution_id: Uuid,
    ) -> Result<ExecutionStatus, LogServiceError> {
        // For remote executions, we can't easily determine status
        // without making an additional API call. Return Unknown for now.
        Ok(ExecutionStatus::Unknown)
    }
}

/// Information about where an execution is located.
#[derive(Debug, Clone)]
pub struct ExecutionLocation {
    /// True if the execution is on this node, false if remote.
    pub is_local: bool,
    /// For remote executions, the node URL to proxy to.
    pub remote_node_url: Option<String>,
    /// For remote executions, the target node ID.
    pub remote_node_id: Option<Uuid>,
}

impl ExecutionLocation {
    /// Create a local execution location.
    pub fn local() -> Self {
        Self {
            is_local: true,
            remote_node_url: None,
            remote_node_id: None,
        }
    }

    /// Create a remote execution location.
    pub fn remote(node_url: String, node_id: Uuid) -> Self {
        Self {
            is_local: false,
            remote_node_url: Some(node_url),
            remote_node_id: Some(node_id),
        }
    }
}

/// Unified log service that routes requests to local or remote services.
///
/// This service determines whether an execution is local or remote and
/// dispatches log requests accordingly.
#[derive(Debug, Clone)]
pub struct UnifiedLogService {
    local_service: LocalLogService,
    remote_service: RemoteLogService,
}

impl UnifiedLogService {
    /// Create a new UnifiedLogService.
    pub fn new(pool: SqlitePool, proxy_client: NodeProxyClient) -> Self {
        Self {
            local_service: LocalLogService::new(pool),
            remote_service: RemoteLogService::new(proxy_client),
        }
    }

    /// Determine the location of an execution.
    ///
    /// This checks if the execution exists in the local database. If it does,
    /// the execution is local. Otherwise, we would need to check remote nodes.
    ///
    /// Note: In a full implementation, this would also check shared_task_attempts
    /// to determine if this is a remote execution from another node.
    pub async fn get_execution_location(
        &self,
        execution_id: Uuid,
    ) -> Result<ExecutionLocation, LogServiceError> {
        use db::models::execution_process::ExecutionProcess;

        let process = ExecutionProcess::find_by_id(&self.local_service.pool, execution_id).await?;

        if process.is_some() {
            Ok(ExecutionLocation::local())
        } else {
            // Execution not found locally - it could be remote or not exist at all.
            // For now, return an error. In a full implementation, we would check
            // shared_task_attempts to find the remote node.
            Err(LogServiceError::ExecutionNotFound(execution_id))
        }
    }

    /// Get paginated logs, automatically routing to local or remote service.
    pub async fn get_logs_paginated(
        &self,
        execution_id: Uuid,
        cursor: Option<i64>,
        limit: i64,
        direction: Direction,
    ) -> Result<PaginatedLogs, LogServiceError> {
        let location = self.get_execution_location(execution_id).await?;

        if location.is_local {
            self.local_service
                .get_logs_paginated(execution_id, cursor, limit, direction)
                .await
        } else {
            self.remote_service
                .get_logs_paginated(execution_id, cursor, limit, direction)
                .await
        }
    }

    /// Get execution status, automatically routing to local or remote service.
    pub async fn get_execution_status(
        &self,
        execution_id: Uuid,
    ) -> Result<ExecutionStatus, LogServiceError> {
        let location = self.get_execution_location(execution_id).await?;

        if location.is_local {
            self.local_service.get_execution_status(execution_id).await
        } else {
            self.remote_service.get_execution_status(execution_id).await
        }
    }

    /// Get access to the local service directly.
    pub fn local(&self) -> &LocalLogService {
        &self.local_service
    }

    /// Get access to the remote service directly.
    pub fn remote(&self) -> &RemoteLogService {
        &self.remote_service
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test helper to create a mock pool (will be skipped in tests that need real DB)
    // Note: For integration tests, we would use sqlx's test fixtures.

    #[test]
    fn test_execution_location_local() {
        let location = ExecutionLocation::local();
        assert!(location.is_local);
        assert!(location.remote_node_url.is_none());
        assert!(location.remote_node_id.is_none());
    }

    #[test]
    fn test_execution_location_remote() {
        let node_id = Uuid::new_v4();
        let location = ExecutionLocation::remote("https://node.example.com".to_string(), node_id);
        assert!(!location.is_local);
        assert_eq!(
            location.remote_node_url,
            Some("https://node.example.com".to_string())
        );
        assert_eq!(location.remote_node_id, Some(node_id));
    }

    #[test]
    fn test_execution_status_variants() {
        assert_eq!(ExecutionStatus::Running, ExecutionStatus::Running);
        assert_eq!(ExecutionStatus::Completed, ExecutionStatus::Completed);
        assert_eq!(ExecutionStatus::Unknown, ExecutionStatus::Unknown);
        assert_ne!(ExecutionStatus::Running, ExecutionStatus::Completed);
    }

    #[test]
    fn test_log_service_error_display() {
        let err = LogServiceError::ExecutionNotFound(Uuid::nil());
        assert!(err.to_string().contains("Execution not found"));

        let err = LogServiceError::InvalidLocation;
        assert!(err.to_string().contains("Invalid execution location"));
    }

    // Integration tests would go here with actual database fixtures
    // These are marked with #[sqlx::test] and require a test database

    #[test]
    fn test_local_log_service_creation() {
        // This test just verifies the type structure compiles correctly
        // Actual integration tests would use sqlx::test attribute
        fn _accepts_pool(pool: SqlitePool) -> LocalLogService {
            LocalLogService::new(pool)
        }
    }

    #[test]
    fn test_remote_log_service_creation() {
        let proxy_client = NodeProxyClient::disabled();
        let service = RemoteLogService::new(proxy_client);
        // Just verify it compiles and the service is created
        let _ = format!("{:?}", service);
    }

    #[test]
    fn test_unified_log_service_has_accessors() {
        // This is a compile-time test to ensure the public API is correct
        fn _check_api(service: &UnifiedLogService) -> (&LocalLogService, &RemoteLogService) {
            (service.local(), service.remote())
        }
    }
}
