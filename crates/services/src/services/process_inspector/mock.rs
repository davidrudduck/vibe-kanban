//! Mock implementation of ProcessInspector for testing.
//!
//! This implementation maintains an in-memory list of processes that can be
//! manipulated for testing purposes.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;

use super::{ProcessInspector, ProcessInspectorError, RawProcessInfo};

/// Mock implementation of ProcessInspector for testing.
///
/// Stores processes in memory and allows adding/removing them programmatically.
#[derive(Clone)]
pub struct MockProcessInspector {
    processes: Arc<RwLock<HashMap<u32, RawProcessInfo>>>,
}

impl Default for MockProcessInspector {
    fn default() -> Self {
        Self::new()
    }
}

impl MockProcessInspector {
    /// Create a new empty MockProcessInspector.
    pub fn new() -> Self {
        Self {
            processes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a process to the mock.
    pub fn add_process(&self, process: RawProcessInfo) {
        let mut processes = self.processes.write().unwrap();
        processes.insert(process.pid, process);
    }

    /// Remove a process from the mock by PID.
    pub fn remove_process(&self, pid: u32) -> Option<RawProcessInfo> {
        let mut processes = self.processes.write().unwrap();
        processes.remove(&pid)
    }

    /// Clear all processes.
    pub fn clear(&self) {
        let mut processes = self.processes.write().unwrap();
        processes.clear();
    }

    /// Get the current process count.
    pub fn process_count(&self) -> usize {
        let processes = self.processes.read().unwrap();
        processes.len()
    }

    /// Build a tree of descendant PIDs for a given root PID.
    fn find_descendants(&self, root_pid: u32) -> Vec<u32> {
        let processes = self.processes.read().unwrap();

        // Find all children recursively
        let mut descendants = Vec::new();
        let mut to_visit = vec![root_pid];

        while let Some(current_pid) = to_visit.pop() {
            for process in processes.values() {
                if process.parent_pid == Some(current_pid)
                    && process.pid != root_pid
                    && !descendants.contains(&process.pid)
                {
                    descendants.push(process.pid);
                    to_visit.push(process.pid);
                }
            }
        }

        descendants
    }
}

#[async_trait]
impl ProcessInspector for MockProcessInspector {
    async fn list_processes(&self) -> Result<Vec<RawProcessInfo>, ProcessInspectorError> {
        let processes = self.processes.read().unwrap();
        Ok(processes.values().cloned().collect())
    }

    async fn get_process_tree(
        &self,
        root_pid: u32,
    ) -> Result<Vec<RawProcessInfo>, ProcessInspectorError> {
        let descendant_pids = self.find_descendants(root_pid);
        let processes = self.processes.read().unwrap();

        let descendants = descendant_pids
            .iter()
            .filter_map(|pid| processes.get(pid).cloned())
            .collect();

        Ok(descendants)
    }

    async fn find_processes_by_cwd_prefix(
        &self,
        cwd_prefix: &str,
    ) -> Result<Vec<RawProcessInfo>, ProcessInspectorError> {
        let processes = self.processes.read().unwrap();

        let matches = processes
            .values()
            .filter(|p| {
                p.working_directory
                    .as_ref()
                    .map(|cwd| cwd.starts_with(cwd_prefix))
                    .unwrap_or(false)
            })
            .cloned()
            .collect();

        Ok(matches)
    }

    async fn kill_process(&self, pid: u32, _force: bool) -> Result<(), ProcessInspectorError> {
        let mut processes = self.processes.write().unwrap();

        if processes.remove(&pid).is_some() {
            Ok(())
        } else {
            Err(ProcessInspectorError::ProcessNotFound(pid))
        }
    }

    async fn process_exists(&self, pid: u32) -> bool {
        let processes = self.processes.read().unwrap();
        processes.contains_key(&pid)
    }
}
