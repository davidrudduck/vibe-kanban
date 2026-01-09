//! Shared proxy checking logic for remote project and task attempt operations.
//!
//! This module provides utilities for checking if a request should be proxied
//! to a remote node based on the remote context (project or task attempt).

use crate::error::ApiError;
use crate::middleware::{RemoteProjectContext, RemoteTaskAttemptContext};
use uuid::Uuid;

/// Information needed to proxy a request to a remote node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteProxyInfo {
    /// The public URL of the remote node (e.g., "https://node.example.com")
    pub node_url: String,
    /// The UUID of the remote node
    pub node_id: Uuid,
    /// The target ID for routing (remote_project_id for projects, task_id for task attempts)
    pub target_id: Uuid,
}

/// Check if a remote project context is available and online.
///
/// Returns `Ok(Some(RemoteProxyInfo))` if the request should be proxied to a remote node,
/// `Ok(None)` if no remote context is present (local operation),
/// or `Err(ApiError)` if the remote node is offline or has no URL configured.
///
/// # Arguments
/// * `remote_ctx` - Optional reference to the remote project context
///
/// # Returns
/// * `Ok(Some(info))` - Proxy to remote node using the provided info
/// * `Ok(None)` - No proxy needed, handle locally
/// * `Err(ApiError::BadGateway)` - Remote node is offline or has no URL
pub fn check_remote_proxy(
    remote_ctx: Option<&RemoteProjectContext>,
) -> Result<Option<RemoteProxyInfo>, ApiError> {
    match remote_ctx {
        Some(ctx) => {
            // Check if the node is online
            if ctx.node_status.as_deref() != Some("online") {
                return Err(ApiError::BadGateway(format!(
                    "Remote node '{}' is offline",
                    ctx.node_id
                )));
            }

            // Check if we have a URL to proxy to
            let node_url = ctx.node_url.as_ref().ok_or_else(|| {
                ApiError::BadGateway(format!(
                    "Remote node '{}' has no public URL configured",
                    ctx.node_id
                ))
            })?;

            Ok(Some(RemoteProxyInfo {
                node_url: node_url.clone(),
                node_id: ctx.node_id,
                target_id: ctx.remote_project_id,
            }))
        }
        None => Ok(None),
    }
}

/// Check if a remote task attempt context is available and online.
///
/// Returns `Ok(Some(RemoteProxyInfo))` if the request should be proxied to a remote node,
/// `Ok(None)` if no remote context is present (local operation),
/// or `Err(ApiError)` if the remote node is offline or has no URL configured.
///
/// # Arguments
/// * `remote_ctx` - Optional reference to the remote task attempt context
///
/// # Returns
/// * `Ok(Some(info))` - Proxy to remote node using the provided info
/// * `Ok(None)` - No proxy needed, handle locally
/// * `Err(ApiError::BadGateway)` - Remote node is offline or has no URL
pub fn check_remote_task_attempt_proxy(
    remote_ctx: Option<&RemoteTaskAttemptContext>,
) -> Result<Option<RemoteProxyInfo>, ApiError> {
    match remote_ctx {
        Some(ctx) => {
            // Check if the node is online
            if ctx.node_status.as_deref() != Some("online") {
                return Err(ApiError::BadGateway(format!(
                    "Remote node '{}' is offline",
                    ctx.node_id
                )));
            }

            // Check if we have a URL to proxy to
            let node_url = ctx.node_url.as_ref().ok_or_else(|| {
                ApiError::BadGateway(format!(
                    "Remote node '{}' has no public URL configured",
                    ctx.node_id
                ))
            })?;

            Ok(Some(RemoteProxyInfo {
                node_url: node_url.clone(),
                node_id: ctx.node_id,
                target_id: ctx.task_id,
            }))
        }
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // Tests for check_remote_proxy (RemoteProjectContext)
    // ==========================================================================

    #[test]
    fn test_check_remote_proxy_none() {
        // When no remote context is provided, should return Ok(None)
        let result = check_remote_proxy(None);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_check_remote_proxy_with_context_online() {
        // When context is present and node is online, should return RemoteProxyInfo
        let node_id = Uuid::new_v4();
        let remote_project_id = Uuid::new_v4();
        let ctx = RemoteProjectContext {
            node_id,
            node_url: Some("http://node:3000".to_string()),
            node_status: Some("online".to_string()),
            remote_project_id,
        };

        let result = check_remote_proxy(Some(&ctx));
        assert!(result.is_ok());

        let proxy_info = result.unwrap();
        assert!(proxy_info.is_some());

        let info = proxy_info.unwrap();
        assert_eq!(info.node_url, "http://node:3000");
        assert_eq!(info.node_id, node_id);
        assert_eq!(info.target_id, remote_project_id);
    }

    #[test]
    fn test_check_remote_proxy_returns_error_when_node_offline() {
        // When node is offline, should return BadGateway error
        let ctx = RemoteProjectContext {
            node_id: Uuid::new_v4(),
            node_url: Some("http://node:3000".to_string()),
            node_status: Some("offline".to_string()),
            remote_project_id: Uuid::new_v4(),
        };

        let result = check_remote_proxy(Some(&ctx));
        assert!(result.is_err());
        match result {
            Err(ApiError::BadGateway(msg)) => {
                assert!(msg.contains("offline"));
            }
            _ => panic!("Expected BadGateway error"),
        }
    }

    #[test]
    fn test_check_remote_proxy_returns_error_when_no_node_url() {
        // When node URL is None, should return BadGateway error
        let ctx = RemoteProjectContext {
            node_id: Uuid::new_v4(),
            node_url: None,
            node_status: Some("online".to_string()),
            remote_project_id: Uuid::new_v4(),
        };

        let result = check_remote_proxy(Some(&ctx));
        assert!(result.is_err());
        match result {
            Err(ApiError::BadGateway(msg)) => {
                assert!(msg.contains("no public URL"));
            }
            _ => panic!("Expected BadGateway error"),
        }
    }

    #[test]
    fn test_check_remote_proxy_returns_error_when_node_status_none() {
        // When node status is None (not "online"), should return BadGateway error
        let ctx = RemoteProjectContext {
            node_id: Uuid::new_v4(),
            node_url: Some("http://node:3000".to_string()),
            node_status: None,
            remote_project_id: Uuid::new_v4(),
        };

        let result = check_remote_proxy(Some(&ctx));
        assert!(result.is_err());
        match result {
            Err(ApiError::BadGateway(msg)) => {
                assert!(msg.contains("offline"));
            }
            _ => panic!("Expected BadGateway error"),
        }
    }

    // ==========================================================================
    // Tests for check_remote_task_attempt_proxy (RemoteTaskAttemptContext)
    // ==========================================================================

    #[test]
    fn test_check_remote_task_attempt_proxy_none() {
        // When no remote context is provided, should return Ok(None)
        let result = check_remote_task_attempt_proxy(None);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_check_remote_task_attempt_proxy_with_context_online() {
        // When context is present and node is online, should return RemoteProxyInfo
        let node_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let ctx = RemoteTaskAttemptContext {
            node_id,
            node_url: Some("http://node:3000".to_string()),
            node_status: Some("online".to_string()),
            task_id,
        };

        let result = check_remote_task_attempt_proxy(Some(&ctx));
        assert!(result.is_ok());

        let proxy_info = result.unwrap();
        assert!(proxy_info.is_some());

        let info = proxy_info.unwrap();
        assert_eq!(info.node_url, "http://node:3000");
        assert_eq!(info.node_id, node_id);
        assert_eq!(info.target_id, task_id);
    }

    #[test]
    fn test_check_remote_task_attempt_proxy_returns_error_when_node_offline() {
        // When node is offline, should return BadGateway error
        let ctx = RemoteTaskAttemptContext {
            node_id: Uuid::new_v4(),
            node_url: Some("http://node:3000".to_string()),
            node_status: Some("offline".to_string()),
            task_id: Uuid::new_v4(),
        };

        let result = check_remote_task_attempt_proxy(Some(&ctx));
        assert!(result.is_err());
        match result {
            Err(ApiError::BadGateway(msg)) => {
                assert!(msg.contains("offline"));
            }
            _ => panic!("Expected BadGateway error"),
        }
    }

    #[test]
    fn test_check_remote_task_attempt_proxy_returns_error_when_no_node_url() {
        // When node URL is None, should return BadGateway error
        let ctx = RemoteTaskAttemptContext {
            node_id: Uuid::new_v4(),
            node_url: None,
            node_status: Some("online".to_string()),
            task_id: Uuid::new_v4(),
        };

        let result = check_remote_task_attempt_proxy(Some(&ctx));
        assert!(result.is_err());
        match result {
            Err(ApiError::BadGateway(msg)) => {
                assert!(msg.contains("no public URL"));
            }
            _ => panic!("Expected BadGateway error"),
        }
    }

    #[test]
    fn test_check_remote_task_attempt_proxy_returns_error_when_node_status_none() {
        // When node status is None (not "online"), should return BadGateway error
        let ctx = RemoteTaskAttemptContext {
            node_id: Uuid::new_v4(),
            node_url: Some("http://node:3000".to_string()),
            node_status: None,
            task_id: Uuid::new_v4(),
        };

        let result = check_remote_task_attempt_proxy(Some(&ctx));
        assert!(result.is_err());
        match result {
            Err(ApiError::BadGateway(msg)) => {
                assert!(msg.contains("offline"));
            }
            _ => panic!("Expected BadGateway error"),
        }
    }
}
