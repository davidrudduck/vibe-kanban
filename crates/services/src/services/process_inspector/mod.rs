//! Process inspection and management for vibe-kanban.
//!
//! This module provides a trait-based abstraction for process inspection,
//! allowing platform-specific implementations and mock implementations for testing.

mod mock;

pub use mock::MockProcessInspector;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

/// Error types for process inspection operations.
#[derive(Debug, Error)]
pub enum ProcessInspectorError {
    #[error("Process not found: PID {0}")]
    ProcessNotFound(u32),
    #[error("Permission denied to access process: PID {0}")]
    PermissionDenied(u32),
    #[error("Failed to kill process PID {pid}: {message}")]
    KillFailed { pid: u32, message: String },
    #[error("System error: {0}")]
    SystemError(String),
}

/// Information about a single system process.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RawProcessInfo {
    /// Process ID
    pub pid: u32,
    /// Parent process ID (None for orphans/init)
    pub parent_pid: Option<u32>,
    /// Process name (executable name)
    pub name: String,
    /// Full command line arguments
    pub command: Vec<String>,
    /// Working directory of the process
    pub working_directory: Option<String>,
    /// Memory usage in bytes
    pub memory_bytes: u64,
    /// CPU usage percentage (0.0 - 100.0)
    pub cpu_percent: f32,
}

impl RawProcessInfo {
    /// Create a new RawProcessInfo
    pub fn new(
        pid: u32,
        parent_pid: Option<u32>,
        name: String,
        command: Vec<String>,
        working_directory: Option<String>,
        memory_bytes: u64,
        cpu_percent: f32,
    ) -> Self {
        Self {
            pid,
            parent_pid,
            name,
            command,
            working_directory,
            memory_bytes,
            cpu_percent,
        }
    }
}

/// Trait for platform-abstracted process inspection.
///
/// This trait defines operations for:
/// - Listing all processes
/// - Finding processes in a process tree (descendants of a PID)
/// - Finding processes by working directory prefix
/// - Killing processes
#[async_trait]
pub trait ProcessInspector: Send + Sync {
    /// List all processes on the system.
    async fn list_processes(&self) -> Result<Vec<RawProcessInfo>, ProcessInspectorError>;

    /// Get all descendant processes of the given PID (process tree).
    ///
    /// Returns all processes where this PID is an ancestor (direct or indirect parent).
    /// Does not include the process with the given PID itself.
    async fn get_process_tree(
        &self,
        root_pid: u32,
    ) -> Result<Vec<RawProcessInfo>, ProcessInspectorError>;

    /// Find all processes whose working directory starts with the given prefix.
    ///
    /// This is used to find processes running in a specific worktree directory.
    async fn find_processes_by_cwd_prefix(
        &self,
        cwd_prefix: &str,
    ) -> Result<Vec<RawProcessInfo>, ProcessInspectorError>;

    /// Kill a process by PID.
    ///
    /// If `force` is true, sends SIGKILL (immediate termination).
    /// If `force` is false, sends SIGTERM (graceful termination).
    async fn kill_process(&self, pid: u32, force: bool) -> Result<(), ProcessInspectorError>;

    /// Check if a process with the given PID exists.
    async fn process_exists(&self, pid: u32) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_list_processes() {
        let mock = MockProcessInspector::new();

        // Add some processes
        mock.add_process(RawProcessInfo::new(
            1000,
            Some(1),
            "node".to_string(),
            vec!["node".to_string(), "server.js".to_string()],
            Some("/home/user/project".to_string()),
            1024 * 1024 * 50, // 50 MB
            2.5,
        ));

        mock.add_process(RawProcessInfo::new(
            1001,
            Some(1000),
            "npm".to_string(),
            vec!["npm".to_string(), "run".to_string(), "watch".to_string()],
            Some("/home/user/project".to_string()),
            1024 * 1024 * 30, // 30 MB
            1.0,
        ));

        let processes = mock.list_processes().await.unwrap();
        assert_eq!(processes.len(), 2);

        // Verify first process
        let node_process = processes.iter().find(|p| p.pid == 1000).unwrap();
        assert_eq!(node_process.name, "node");
        assert_eq!(node_process.parent_pid, Some(1));
    }

    #[tokio::test]
    async fn test_mock_process_tree_returns_descendants() {
        let mock = MockProcessInspector::new();

        // Build a process tree:
        // PID 100 (root)
        //   └── PID 200 (child)
        //       └── PID 300 (grandchild)
        //   └── PID 201 (child)
        // PID 500 (unrelated)

        mock.add_process(RawProcessInfo::new(
            100,
            Some(1),
            "executor".to_string(),
            vec!["claude-code".to_string()],
            Some("/tmp/worktree".to_string()),
            1024 * 1024,
            1.0,
        ));

        mock.add_process(RawProcessInfo::new(
            200,
            Some(100),
            "node".to_string(),
            vec!["node".to_string()],
            Some("/tmp/worktree".to_string()),
            1024 * 1024,
            1.0,
        ));

        mock.add_process(RawProcessInfo::new(
            300,
            Some(200),
            "npm".to_string(),
            vec!["npm".to_string()],
            Some("/tmp/worktree".to_string()),
            1024 * 1024,
            1.0,
        ));

        mock.add_process(RawProcessInfo::new(
            201,
            Some(100),
            "git".to_string(),
            vec!["git".to_string()],
            Some("/tmp/worktree".to_string()),
            1024 * 1024,
            1.0,
        ));

        mock.add_process(RawProcessInfo::new(
            500,
            Some(1),
            "unrelated".to_string(),
            vec!["unrelated".to_string()],
            Some("/home/user".to_string()),
            1024 * 1024,
            1.0,
        ));

        // Get descendants of PID 100
        let descendants = mock.get_process_tree(100).await.unwrap();
        assert_eq!(descendants.len(), 3); // 200, 201, 300

        let pids: Vec<u32> = descendants.iter().map(|p| p.pid).collect();
        assert!(pids.contains(&200));
        assert!(pids.contains(&201));
        assert!(pids.contains(&300));
        assert!(!pids.contains(&100)); // Root not included
        assert!(!pids.contains(&500)); // Unrelated not included
    }

    #[tokio::test]
    async fn test_mock_processes_by_cwd_prefix() {
        let mock = MockProcessInspector::new();

        mock.add_process(RawProcessInfo::new(
            1000,
            Some(1),
            "node".to_string(),
            vec!["node".to_string()],
            Some("/var/worktrees/project-a".to_string()),
            1024 * 1024,
            1.0,
        ));

        mock.add_process(RawProcessInfo::new(
            1001,
            Some(1),
            "npm".to_string(),
            vec!["npm".to_string()],
            Some("/var/worktrees/project-a/subdir".to_string()),
            1024 * 1024,
            1.0,
        ));

        mock.add_process(RawProcessInfo::new(
            1002,
            Some(1),
            "cargo".to_string(),
            vec!["cargo".to_string()],
            Some("/var/worktrees/project-b".to_string()),
            1024 * 1024,
            1.0,
        ));

        mock.add_process(RawProcessInfo::new(
            1003,
            Some(1),
            "vim".to_string(),
            vec!["vim".to_string()],
            None, // No cwd
            1024 * 1024,
            1.0,
        ));

        // Find processes in project-a
        let matches = mock
            .find_processes_by_cwd_prefix("/var/worktrees/project-a")
            .await
            .unwrap();
        assert_eq!(matches.len(), 2);

        let pids: Vec<u32> = matches.iter().map(|p| p.pid).collect();
        assert!(pids.contains(&1000));
        assert!(pids.contains(&1001));
    }

    #[tokio::test]
    async fn test_mock_kill_process() {
        let mock = MockProcessInspector::new();

        mock.add_process(RawProcessInfo::new(
            1000,
            Some(1),
            "node".to_string(),
            vec!["node".to_string()],
            Some("/tmp".to_string()),
            1024 * 1024,
            1.0,
        ));

        // Verify process exists
        assert!(mock.process_exists(1000).await);

        // Kill the process
        mock.kill_process(1000, false).await.unwrap();

        // Verify process is gone
        assert!(!mock.process_exists(1000).await);

        // Killing non-existent process should fail
        let result = mock.kill_process(1000, false).await;
        assert!(matches!(
            result,
            Err(ProcessInspectorError::ProcessNotFound(1000))
        ));
    }

    #[tokio::test]
    async fn test_mock_kill_process_force() {
        let mock = MockProcessInspector::new();

        mock.add_process(RawProcessInfo::new(
            2000,
            Some(1),
            "stubborn".to_string(),
            vec!["stubborn".to_string()],
            Some("/tmp".to_string()),
            1024 * 1024,
            1.0,
        ));

        // Force kill should also work
        mock.kill_process(2000, true).await.unwrap();
        assert!(!mock.process_exists(2000).await);
    }

    #[tokio::test]
    async fn test_process_tree_empty_for_nonexistent() {
        let mock = MockProcessInspector::new();

        // Get tree for non-existent process should return empty (not error)
        let descendants = mock.get_process_tree(9999).await.unwrap();
        assert!(descendants.is_empty());
    }

    #[tokio::test]
    async fn test_find_processes_by_cwd_prefix_empty() {
        let mock = MockProcessInspector::new();

        mock.add_process(RawProcessInfo::new(
            1000,
            Some(1),
            "node".to_string(),
            vec!["node".to_string()],
            Some("/home/user/project".to_string()),
            1024 * 1024,
            1.0,
        ));

        // No processes in /var/worktrees
        let matches = mock
            .find_processes_by_cwd_prefix("/var/worktrees")
            .await
            .unwrap();
        assert!(matches.is_empty());
    }
}
