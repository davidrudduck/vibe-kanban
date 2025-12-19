//! Process service for vibe-kanban process management.
//!
//! This service discovers system processes and associates them with vibe-kanban
//! entities (projects, tasks, execution processes) for management purposes.

use std::sync::Arc;

use db::models::{
    execution_process::ExecutionProcess, project::Project, task::Task, task_attempt::TaskAttempt,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use thiserror::Error;
use ts_rs::TS;
use uuid::Uuid;

use super::process_inspector::{ProcessInspector, ProcessInspectorError, RawProcessInfo};

/// Error types for process service operations.
#[derive(Debug, Error)]
pub enum ProcessServiceError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    ProcessInspector(#[from] ProcessInspectorError),
    #[error("No processes found matching criteria")]
    NoProcessesFound,
}

/// Information about a process with vibe-kanban association context.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProcessInfo {
    /// System process ID
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
    // Association context
    /// ID of the execution process record (if this is a tracked executor)
    pub execution_process_id: Option<Uuid>,
    /// ID of the task attempt this process belongs to
    pub task_attempt_id: Option<Uuid>,
    /// ID of the task this process belongs to
    pub task_id: Option<Uuid>,
    /// ID of the project this process belongs to
    pub project_id: Option<Uuid>,
    /// Name of the project (for display)
    pub project_name: Option<String>,
    /// Title of the task (for display)
    pub task_title: Option<String>,
    /// Whether this is a direct executor process (vs a child spawned by an executor)
    pub is_executor: bool,
}

/// Association data for linking a process to vibe-kanban entities.
#[derive(Debug, Clone, Default)]
struct ProcessAssociation {
    execution_process_id: Option<Uuid>,
    task_attempt_id: Option<Uuid>,
    task_id: Option<Uuid>,
    project_id: Option<Uuid>,
    project_name: Option<String>,
    task_title: Option<String>,
    is_executor: bool,
}

impl ProcessInfo {
    /// Create a ProcessInfo from RawProcessInfo with no associations
    fn from_raw(raw: RawProcessInfo) -> Self {
        Self {
            pid: raw.pid,
            parent_pid: raw.parent_pid,
            name: raw.name,
            command: raw.command,
            working_directory: raw.working_directory,
            memory_bytes: raw.memory_bytes,
            cpu_percent: raw.cpu_percent,
            execution_process_id: None,
            task_attempt_id: None,
            task_id: None,
            project_id: None,
            project_name: None,
            task_title: None,
            is_executor: false,
        }
    }

    /// Add association context to a ProcessInfo
    fn with_association(mut self, assoc: ProcessAssociation) -> Self {
        self.execution_process_id = assoc.execution_process_id;
        self.task_attempt_id = assoc.task_attempt_id;
        self.task_id = assoc.task_id;
        self.project_id = assoc.project_id;
        self.project_name = assoc.project_name;
        self.task_title = assoc.task_title;
        self.is_executor = assoc.is_executor;
        self
    }
}

/// Filter criteria for listing processes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProcessFilter {
    /// Filter by project ID
    pub project_id: Option<Uuid>,
    /// Filter by task ID
    pub task_id: Option<Uuid>,
    /// Filter by task attempt ID
    pub task_attempt_id: Option<Uuid>,
    /// Only include executor processes (exclude children)
    pub executors_only: bool,
}

/// Scope for kill operations.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum KillScope {
    /// Kill a single process by PID
    Single { pid: u32 },
    /// Kill all processes for a task
    Task { task_id: Uuid },
    /// Kill all processes for a project
    Project { project_id: Uuid },
    /// Kill all vibe-kanban processes except executors
    AllExceptExecutors,
    /// Kill all vibe-kanban processes including executors
    All,
}

/// Result of a kill operation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct KillResult {
    /// Number of processes successfully killed
    pub killed_count: u32,
    /// Number of processes that failed to kill
    pub failed_count: u32,
    /// PIDs that failed to kill with error messages
    pub failures: Vec<KillFailure>,
}

/// Details about a failed kill operation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct KillFailure {
    /// PID that failed to kill
    pub pid: u32,
    /// Error message
    pub error: String,
}

/// Cached association data for a running execution.
#[derive(Debug, Clone)]
struct ExecutionAssociation {
    execution_process_id: Uuid,
    pid: u32,
    task_attempt_id: Uuid,
    task_id: Uuid,
    project_id: Uuid,
    project_name: String,
    task_title: String,
    worktree_path: Option<String>,
}

/// Process service for discovering and managing vibe-kanban processes.
pub struct ProcessService<I: ProcessInspector> {
    inspector: Arc<I>,
}

impl<I: ProcessInspector> Clone for ProcessService<I> {
    fn clone(&self) -> Self {
        Self {
            inspector: Arc::clone(&self.inspector),
        }
    }
}

impl<I: ProcessInspector> ProcessService<I> {
    /// Create a new ProcessService with the given inspector.
    pub fn new(inspector: I) -> Self {
        Self {
            inspector: Arc::new(inspector),
        }
    }

    /// List all vibe-kanban related processes with association information.
    ///
    /// This discovers processes via:
    /// 1. Executor PIDs stored in the database (process tree)
    /// 2. Working directory matching worktree paths
    pub async fn list_processes(
        &self,
        pool: &SqlitePool,
        filter: Option<ProcessFilter>,
    ) -> Result<Vec<ProcessInfo>, ProcessServiceError> {
        let filter = filter.unwrap_or_default();

        // Step 1: Get running execution processes with PIDs from the database
        let associations = self.load_execution_associations(pool).await?;

        // Step 2: For each executor PID, get the process tree
        let mut all_processes: Vec<ProcessInfo> = Vec::new();
        let mut seen_pids = std::collections::HashSet::new();

        for assoc in &associations {
            // Check if process still exists
            if !self.inspector.process_exists(assoc.pid).await {
                continue;
            }

            // Build base association for child processes
            let child_assoc = ProcessAssociation {
                execution_process_id: None,
                task_attempt_id: Some(assoc.task_attempt_id),
                task_id: Some(assoc.task_id),
                project_id: Some(assoc.project_id),
                project_name: Some(assoc.project_name.clone()),
                task_title: Some(assoc.task_title.clone()),
                is_executor: false,
            };

            // Get the executor process itself
            if let Ok(procs) = self.inspector.list_processes().await
                && let Some(exec_proc) = procs.iter().find(|p| p.pid == assoc.pid)
                && !seen_pids.contains(&exec_proc.pid)
            {
                seen_pids.insert(exec_proc.pid);
                let executor_assoc = ProcessAssociation {
                    execution_process_id: Some(assoc.execution_process_id),
                    is_executor: true,
                    ..child_assoc.clone()
                };
                let info =
                    ProcessInfo::from_raw(exec_proc.clone()).with_association(executor_assoc);
                all_processes.push(info);
            }

            // Get all descendant processes
            if let Ok(descendants) = self.inspector.get_process_tree(assoc.pid).await {
                for raw in descendants {
                    if !seen_pids.contains(&raw.pid) {
                        seen_pids.insert(raw.pid);
                        let info = ProcessInfo::from_raw(raw).with_association(child_assoc.clone());
                        all_processes.push(info);
                    }
                }
            }

            // Also find processes by working directory (catches processes started in worktree)
            if let Some(ref worktree) = assoc.worktree_path
                && let Ok(cwd_procs) = self.inspector.find_processes_by_cwd_prefix(worktree).await
            {
                for raw in cwd_procs {
                    if !seen_pids.contains(&raw.pid) {
                        seen_pids.insert(raw.pid);
                        let info = ProcessInfo::from_raw(raw).with_association(child_assoc.clone());
                        all_processes.push(info);
                    }
                }
            }
        }

        // Step 3: Apply filters
        let filtered = all_processes
            .into_iter()
            .filter(|p| {
                if let Some(project_id) = filter.project_id
                    && p.project_id != Some(project_id)
                {
                    return false;
                }
                if let Some(task_id) = filter.task_id
                    && p.task_id != Some(task_id)
                {
                    return false;
                }
                if let Some(attempt_id) = filter.task_attempt_id
                    && p.task_attempt_id != Some(attempt_id)
                {
                    return false;
                }
                if filter.executors_only && !p.is_executor {
                    return false;
                }
                true
            })
            .collect();

        Ok(filtered)
    }

    /// Kill processes based on the specified scope.
    pub async fn kill_processes(
        &self,
        pool: &SqlitePool,
        scope: KillScope,
        force: bool,
    ) -> Result<KillResult, ProcessServiceError> {
        // Get processes matching the scope
        let filter = match &scope {
            KillScope::Single { pid } => {
                // For single PID, we'll handle it directly
                return self.kill_single_process(*pid, force).await;
            }
            KillScope::Task { task_id } => Some(ProcessFilter {
                task_id: Some(*task_id),
                ..Default::default()
            }),
            KillScope::Project { project_id } => Some(ProcessFilter {
                project_id: Some(*project_id),
                ..Default::default()
            }),
            KillScope::AllExceptExecutors => Some(ProcessFilter {
                executors_only: false,
                ..Default::default()
            }),
            KillScope::All => None,
        };

        let mut processes = self.list_processes(pool, filter).await?;

        // For AllExceptExecutors, filter out executors
        if matches!(scope, KillScope::AllExceptExecutors) {
            processes.retain(|p| !p.is_executor);
        }

        // Kill processes in reverse order (children first, then parents)
        // This helps with proper cleanup
        processes.sort_by_key(|p| std::cmp::Reverse(p.pid));

        let mut killed_count = 0u32;
        let mut failures = Vec::new();

        for proc in processes {
            match self.inspector.kill_process(proc.pid, force).await {
                Ok(()) => killed_count += 1,
                Err(ProcessInspectorError::ProcessNotFound(_)) => {
                    // Process already exited, count as success
                    killed_count += 1;
                }
                Err(e) => {
                    failures.push(KillFailure {
                        pid: proc.pid,
                        error: e.to_string(),
                    });
                }
            }
        }

        Ok(KillResult {
            killed_count,
            failed_count: failures.len() as u32,
            failures,
        })
    }

    /// Kill a single process by PID.
    async fn kill_single_process(
        &self,
        pid: u32,
        force: bool,
    ) -> Result<KillResult, ProcessServiceError> {
        match self.inspector.kill_process(pid, force).await {
            Ok(()) => Ok(KillResult {
                killed_count: 1,
                failed_count: 0,
                failures: vec![],
            }),
            Err(ProcessInspectorError::ProcessNotFound(_)) => {
                // Process already exited
                Ok(KillResult {
                    killed_count: 1,
                    failed_count: 0,
                    failures: vec![],
                })
            }
            Err(e) => Ok(KillResult {
                killed_count: 0,
                failed_count: 1,
                failures: vec![KillFailure {
                    pid,
                    error: e.to_string(),
                }],
            }),
        }
    }

    /// Load all running execution processes with their associations.
    async fn load_execution_associations(
        &self,
        pool: &SqlitePool,
    ) -> Result<Vec<ExecutionAssociation>, ProcessServiceError> {
        // Get all running execution processes with PIDs
        let exec_processes = ExecutionProcess::find_running_with_pids(pool).await?;

        let mut associations = Vec::new();

        for ep in exec_processes {
            // Skip if no PID
            let Some(pid) = ep.pid else { continue };

            // Get the task attempt
            let Some(attempt) = TaskAttempt::find_by_id(pool, ep.task_attempt_id).await? else {
                continue;
            };

            // Get the task
            let Some(task) = Task::find_by_id(pool, attempt.task_id).await? else {
                continue;
            };

            // Get the project
            let Some(project) = Project::find_by_id(pool, task.project_id).await? else {
                continue;
            };

            associations.push(ExecutionAssociation {
                execution_process_id: ep.id,
                pid: pid as u32,
                task_attempt_id: attempt.id,
                task_id: task.id,
                project_id: project.id,
                project_name: project.name.clone(),
                task_title: task.title.clone(),
                worktree_path: attempt.container_ref.clone(),
            });
        }

        Ok(associations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::process_inspector::MockProcessInspector;

    /// Helper to create a mock ProcessService for testing
    fn create_mock_service() -> ProcessService<MockProcessInspector> {
        ProcessService::new(MockProcessInspector::new())
    }

    #[test]
    fn test_process_info_from_raw() {
        let raw = RawProcessInfo::new(
            1234,
            Some(1),
            "node".to_string(),
            vec!["node".to_string(), "server.js".to_string()],
            Some("/home/user/project".to_string()),
            1024 * 1024 * 50,
            2.5,
        );

        let info = ProcessInfo::from_raw(raw);

        assert_eq!(info.pid, 1234);
        assert_eq!(info.parent_pid, Some(1));
        assert_eq!(info.name, "node");
        assert_eq!(info.command, vec!["node", "server.js"]);
        assert_eq!(
            info.working_directory,
            Some("/home/user/project".to_string())
        );
        assert_eq!(info.memory_bytes, 1024 * 1024 * 50);
        assert_eq!(info.cpu_percent, 2.5);
        // No associations initially
        assert!(info.execution_process_id.is_none());
        assert!(info.task_attempt_id.is_none());
        assert!(info.task_id.is_none());
        assert!(info.project_id.is_none());
        assert!(info.project_name.is_none());
        assert!(info.task_title.is_none());
        assert!(!info.is_executor);
    }

    #[test]
    fn test_process_info_with_association() {
        let raw = RawProcessInfo::new(
            1234,
            Some(1),
            "node".to_string(),
            vec!["node".to_string()],
            Some("/tmp".to_string()),
            1024,
            1.0,
        );

        let exec_id = Uuid::new_v4();
        let attempt_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();

        let info = ProcessInfo::from_raw(raw).with_association(ProcessAssociation {
            execution_process_id: Some(exec_id),
            task_attempt_id: Some(attempt_id),
            task_id: Some(task_id),
            project_id: Some(project_id),
            project_name: Some("My Project".to_string()),
            task_title: Some("Implement feature".to_string()),
            is_executor: true,
        });

        assert_eq!(info.execution_process_id, Some(exec_id));
        assert_eq!(info.task_attempt_id, Some(attempt_id));
        assert_eq!(info.task_id, Some(task_id));
        assert_eq!(info.project_id, Some(project_id));
        assert_eq!(info.project_name, Some("My Project".to_string()));
        assert_eq!(info.task_title, Some("Implement feature".to_string()));
        assert!(info.is_executor);
    }

    #[test]
    fn test_kill_scope_serialization() {
        // Test Single
        let single = KillScope::Single { pid: 1234 };
        let json = serde_json::to_string(&single).unwrap();
        assert!(json.contains("\"type\":\"single\""));
        assert!(json.contains("\"pid\":1234"));

        // Test Task
        let task_id = Uuid::new_v4();
        let task = KillScope::Task { task_id };
        let json = serde_json::to_string(&task).unwrap();
        assert!(json.contains("\"type\":\"task\""));

        // Test All
        let all = KillScope::All;
        let json = serde_json::to_string(&all).unwrap();
        assert!(json.contains("\"type\":\"all\""));
    }

    #[test]
    fn test_process_filter_default() {
        let filter = ProcessFilter::default();
        assert!(filter.project_id.is_none());
        assert!(filter.task_id.is_none());
        assert!(filter.task_attempt_id.is_none());
        assert!(!filter.executors_only);
    }

    #[tokio::test]
    async fn test_kill_single_process_success() {
        let service = create_mock_service();

        // Add a process to the mock
        service.inspector.add_process(RawProcessInfo::new(
            1234,
            Some(1),
            "test".to_string(),
            vec!["test".to_string()],
            Some("/tmp".to_string()),
            1024,
            1.0,
        ));

        // Verify process exists
        assert!(service.inspector.process_exists(1234).await);

        // Kill it
        let result = service.kill_single_process(1234, false).await.unwrap();
        assert_eq!(result.killed_count, 1);
        assert_eq!(result.failed_count, 0);
        assert!(result.failures.is_empty());

        // Verify process is gone
        assert!(!service.inspector.process_exists(1234).await);
    }

    #[tokio::test]
    async fn test_kill_single_process_not_found() {
        let service = create_mock_service();

        // Try to kill non-existent process - should count as success (already gone)
        let result = service.kill_single_process(9999, false).await.unwrap();
        assert_eq!(result.killed_count, 1);
        assert_eq!(result.failed_count, 0);
    }

    #[tokio::test]
    async fn test_service_is_clone() {
        let service1 = create_mock_service();

        // Add a process
        service1.inspector.add_process(RawProcessInfo::new(
            1000,
            None,
            "test".to_string(),
            vec![],
            None,
            0,
            0.0,
        ));

        // Clone the service
        let service2 = service1.clone();

        // Both should see the same process (shared Arc)
        assert!(service1.inspector.process_exists(1000).await);
        assert!(service2.inspector.process_exists(1000).await);
    }

    #[test]
    fn test_kill_result_structure() {
        let result = KillResult {
            killed_count: 5,
            failed_count: 2,
            failures: vec![
                KillFailure {
                    pid: 1234,
                    error: "Permission denied".to_string(),
                },
                KillFailure {
                    pid: 5678,
                    error: "Signal failed".to_string(),
                },
            ],
        };

        assert_eq!(result.killed_count, 5);
        assert_eq!(result.failed_count, 2);
        assert_eq!(result.failures.len(), 2);
        assert_eq!(result.failures[0].pid, 1234);
        assert_eq!(result.failures[1].error, "Signal failed");
    }
}
