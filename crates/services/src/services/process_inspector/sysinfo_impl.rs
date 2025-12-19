//! Real implementation of ProcessInspector using the sysinfo crate.
//!
//! This implementation uses system APIs (via sysinfo) to inspect processes
//! on Linux and macOS. It provides actual process discovery, tree traversal,
//! working directory matching, and process termination.

use std::collections::HashSet;

use async_trait::async_trait;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use super::{ProcessInspector, ProcessInspectorError, RawProcessInfo};

/// Real implementation of ProcessInspector using the sysinfo crate.
///
/// This implementation queries the operating system for process information
/// and can kill processes using signals (SIGTERM/SIGKILL).
#[derive(Clone)]
pub struct SysinfoProcessInspector {
    // Sysinfo recommends reusing System instances, but for our use case
    // we create a fresh one each time to get current data.
    // The struct is Clone because it has no state.
}

impl Default for SysinfoProcessInspector {
    fn default() -> Self {
        Self::new()
    }
}

impl SysinfoProcessInspector {
    /// Create a new SysinfoProcessInspector.
    pub fn new() -> Self {
        Self {}
    }

    /// Create a fresh System instance and refresh process information.
    fn get_system(&self) -> System {
        let mut system = System::new();
        // Refresh all process information we need
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true, // Update all processes
            ProcessRefreshKind::everything(),
        );
        system
    }

    /// Convert a sysinfo Process to our RawProcessInfo.
    fn process_to_raw_info(pid: Pid, process: &sysinfo::Process) -> RawProcessInfo {
        RawProcessInfo {
            pid: pid.as_u32(),
            parent_pid: process.parent().map(|p| p.as_u32()),
            name: process.name().to_string_lossy().to_string(),
            command: process
                .cmd()
                .iter()
                .map(|s| s.to_string_lossy().to_string())
                .collect(),
            working_directory: process.cwd().map(|p| p.to_string_lossy().to_string()),
            memory_bytes: process.memory(),
            cpu_percent: process.cpu_usage(),
        }
    }
}

#[async_trait]
impl ProcessInspector for SysinfoProcessInspector {
    async fn list_processes(&self) -> Result<Vec<RawProcessInfo>, ProcessInspectorError> {
        let system = self.get_system();

        let processes: Vec<RawProcessInfo> = system
            .processes()
            .iter()
            .map(|(pid, process)| Self::process_to_raw_info(*pid, process))
            .collect();

        Ok(processes)
    }

    async fn get_process_tree(
        &self,
        root_pid: u32,
    ) -> Result<Vec<RawProcessInfo>, ProcessInspectorError> {
        let system = self.get_system();

        // Build a set of all descendant PIDs using BFS
        let mut descendants = HashSet::new();
        let mut to_visit = vec![root_pid];

        while let Some(current_pid) = to_visit.pop() {
            // Find all direct children of current_pid
            for (pid, process) in system.processes() {
                if process.parent().map(|p| p.as_u32()) == Some(current_pid) {
                    let child_pid = pid.as_u32();
                    if child_pid != root_pid && !descendants.contains(&child_pid) {
                        descendants.insert(child_pid);
                        to_visit.push(child_pid);
                    }
                }
            }
        }

        // Convert PIDs to RawProcessInfo
        let result: Vec<RawProcessInfo> = system
            .processes()
            .iter()
            .filter(|(pid, _)| descendants.contains(&pid.as_u32()))
            .map(|(pid, process)| Self::process_to_raw_info(*pid, process))
            .collect();

        Ok(result)
    }

    async fn find_processes_by_cwd_prefix(
        &self,
        cwd_prefix: &str,
    ) -> Result<Vec<RawProcessInfo>, ProcessInspectorError> {
        let system = self.get_system();

        let matches: Vec<RawProcessInfo> = system
            .processes()
            .iter()
            .filter(|(_, process)| {
                process
                    .cwd()
                    .map(|cwd| cwd.to_string_lossy().starts_with(cwd_prefix))
                    .unwrap_or(false)
            })
            .map(|(pid, process)| Self::process_to_raw_info(*pid, process))
            .collect();

        Ok(matches)
    }

    async fn kill_process(&self, pid: u32, force: bool) -> Result<(), ProcessInspectorError> {
        let system = self.get_system();
        let sysinfo_pid = Pid::from_u32(pid);

        // Check if process exists
        let process = system
            .process(sysinfo_pid)
            .ok_or(ProcessInspectorError::ProcessNotFound(pid))?;

        // Use sysinfo's kill method
        // force=true uses SIGKILL, force=false uses SIGTERM
        let signal = if force {
            sysinfo::Signal::Kill
        } else {
            sysinfo::Signal::Term
        };

        if process.kill_with(signal).unwrap_or(false) {
            Ok(())
        } else {
            // Try to get more info about why it failed
            // It might be a permission issue or the process might have exited
            if !self.process_exists(pid).await {
                Err(ProcessInspectorError::ProcessNotFound(pid))
            } else {
                Err(ProcessInspectorError::KillFailed {
                    pid,
                    message: "Failed to send signal (permission denied or process protected)"
                        .to_string(),
                })
            }
        }
    }

    async fn process_exists(&self, pid: u32) -> bool {
        let system = self.get_system();
        let sysinfo_pid = Pid::from_u32(pid);
        system.process(sysinfo_pid).is_some()
    }
}
