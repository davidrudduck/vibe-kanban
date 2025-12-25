//! Validation module for database enums.
//!
//! Provides string-based validation for status fields to replace SQLite CHECK constraints.
//! This is needed for ElectricSQL compatibility since CHECK constraints don't replicate.

use thiserror::Error;

/// Validation errors for status fields
#[derive(Debug, Error, PartialEq)]
pub enum ValidationError {
    #[error(
        "Invalid task status: '{0}'. Valid values: todo, inprogress, inreview, done, cancelled"
    )]
    InvalidTaskStatus(String),

    #[error("Invalid execution status: '{0}'. Valid values: running, completed, failed, killed")]
    InvalidExecutionStatus(String),

    #[error("Invalid node status: '{0}'. Valid values: pending, online, offline, busy, draining")]
    InvalidNodeStatus(String),
}

/// Valid task status values (matches TaskStatus enum serialization)
pub const VALID_TASK_STATUSES: &[&str] = &["todo", "inprogress", "inreview", "done", "cancelled"];

/// Valid execution process status values (matches ExecutionProcessStatus enum serialization)
pub const VALID_EXECUTION_STATUSES: &[&str] = &["running", "completed", "failed", "killed"];

/// Valid node status values (matches CachedNodeStatus enum serialization)
pub const VALID_NODE_STATUSES: &[&str] = &["pending", "online", "offline", "busy", "draining"];

/// Validate a task status string
///
/// # Examples
/// ```
/// use db::validation::validate_task_status;
///
/// assert!(validate_task_status("todo").is_ok());
/// assert!(validate_task_status("invalid").is_err());
/// ```
pub fn validate_task_status(status: &str) -> Result<(), ValidationError> {
    if VALID_TASK_STATUSES.contains(&status) {
        Ok(())
    } else {
        Err(ValidationError::InvalidTaskStatus(status.to_string()))
    }
}

/// Validate an execution process status string
///
/// # Examples
/// ```
/// use db::validation::validate_execution_status;
///
/// assert!(validate_execution_status("running").is_ok());
/// assert!(validate_execution_status("invalid").is_err());
/// ```
pub fn validate_execution_status(status: &str) -> Result<(), ValidationError> {
    if VALID_EXECUTION_STATUSES.contains(&status) {
        Ok(())
    } else {
        Err(ValidationError::InvalidExecutionStatus(status.to_string()))
    }
}

/// Validate a node status string
///
/// # Examples
/// ```
/// use db::validation::validate_node_status;
///
/// assert!(validate_node_status("online").is_ok());
/// assert!(validate_node_status("invalid").is_err());
/// ```
pub fn validate_node_status(status: &str) -> Result<(), ValidationError> {
    if VALID_NODE_STATUSES.contains(&status) {
        Ok(())
    } else {
        Err(ValidationError::InvalidNodeStatus(status.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_task_status_valid() {
        assert!(validate_task_status("todo").is_ok());
        assert!(validate_task_status("inprogress").is_ok());
        assert!(validate_task_status("inreview").is_ok());
        assert!(validate_task_status("done").is_ok());
        assert!(validate_task_status("cancelled").is_ok());
    }

    #[test]
    fn test_validate_task_status_invalid() {
        assert!(validate_task_status("invalid").is_err());
        assert!(validate_task_status("").is_err());
        assert!(validate_task_status("TODO").is_err()); // Case-sensitive
        assert!(validate_task_status("in_progress").is_err()); // Wrong format
        assert!(validate_task_status("in-progress").is_err()); // Wrong format
    }

    #[test]
    fn test_validate_task_status_error_message() {
        let err = validate_task_status("bad").unwrap_err();
        assert_eq!(err, ValidationError::InvalidTaskStatus("bad".to_string()));
        assert!(err.to_string().contains("Invalid task status"));
        assert!(err.to_string().contains("bad"));
    }

    #[test]
    fn test_validate_execution_status_valid() {
        assert!(validate_execution_status("running").is_ok());
        assert!(validate_execution_status("completed").is_ok());
        assert!(validate_execution_status("failed").is_ok());
        assert!(validate_execution_status("killed").is_ok());
    }

    #[test]
    fn test_validate_execution_status_invalid() {
        assert!(validate_execution_status("invalid").is_err());
        assert!(validate_execution_status("").is_err());
        assert!(validate_execution_status("RUNNING").is_err());
        assert!(validate_execution_status("pending").is_err()); // Not an execution status
    }

    #[test]
    fn test_validate_execution_status_error_message() {
        let err = validate_execution_status("bad").unwrap_err();
        assert_eq!(
            err,
            ValidationError::InvalidExecutionStatus("bad".to_string())
        );
        assert!(err.to_string().contains("Invalid execution status"));
    }

    #[test]
    fn test_validate_node_status_valid() {
        assert!(validate_node_status("pending").is_ok());
        assert!(validate_node_status("online").is_ok());
        assert!(validate_node_status("offline").is_ok());
        assert!(validate_node_status("busy").is_ok());
        assert!(validate_node_status("draining").is_ok());
    }

    #[test]
    fn test_validate_node_status_invalid() {
        assert!(validate_node_status("invalid").is_err());
        assert!(validate_node_status("").is_err());
        assert!(validate_node_status("ONLINE").is_err());
        assert!(validate_node_status("running").is_err()); // Not a node status
    }

    #[test]
    fn test_validate_node_status_error_message() {
        let err = validate_node_status("bad").unwrap_err();
        assert_eq!(err, ValidationError::InvalidNodeStatus("bad".to_string()));
        assert!(err.to_string().contains("Invalid node status"));
    }
}
