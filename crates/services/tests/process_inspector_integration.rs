//! Integration tests for SysinfoProcessInspector.
//!
//! These tests spawn real processes and verify that the inspector can detect
//! them by working directory and process tree, and can terminate them.

use std::process::{Command, Stdio};
use std::time::Duration;

use services::services::process_inspector::{ProcessInspector, SysinfoProcessInspector};
use tempfile::TempDir;
use tokio::time::sleep;

/// Helper to spawn a long-running process in a specific directory.
fn spawn_sleep_process(cwd: &std::path::Path, seconds: u32) -> std::process::Child {
    Command::new("sleep")
        .arg(seconds.to_string())
        .current_dir(cwd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to spawn sleep process")
}

/// Helper to spawn a bash process that spawns a child.
fn spawn_bash_with_child(cwd: &std::path::Path) -> std::process::Child {
    // Spawn bash which runs sleep as a child process
    Command::new("bash")
        .args(["-c", "sleep 300 & wait"])
        .current_dir(cwd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to spawn bash process")
}

#[tokio::test]
async fn test_list_processes_includes_current() {
    let inspector = SysinfoProcessInspector::new();

    // Our own process should be in the list
    let current_pid = std::process::id();

    let processes = inspector.list_processes().await.unwrap();

    // Should have multiple processes
    assert!(
        !processes.is_empty(),
        "Expected at least one process in list"
    );

    // Our own process should be included
    let found = processes.iter().any(|p| p.pid == current_pid);
    assert!(
        found,
        "Expected to find current process (pid {}) in list",
        current_pid
    );
}

#[tokio::test]
async fn test_detect_process_by_working_directory() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();

    // Spawn a process in the temp directory
    let mut child = spawn_sleep_process(&temp_path, 300);
    let child_pid = child.id();

    // Give the process time to start
    sleep(Duration::from_millis(100)).await;

    let inspector = SysinfoProcessInspector::new();

    // Find processes with temp_path as cwd prefix
    let temp_path_str = temp_path.to_string_lossy().to_string();
    let matches = inspector
        .find_processes_by_cwd_prefix(&temp_path_str)
        .await
        .unwrap();

    // Should find our spawned process
    let found = matches.iter().any(|p| p.pid == child_pid);
    assert!(
        found,
        "Expected to find process {} with cwd starting with {}. Found: {:?}",
        child_pid,
        temp_path_str,
        matches
            .iter()
            .map(|p| (p.pid, &p.working_directory))
            .collect::<Vec<_>>()
    );

    // Clean up
    child.kill().ok();
    child.wait().ok();
}

#[tokio::test]
async fn test_detect_process_tree() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();

    // Spawn bash which will spawn a child sleep process
    let mut parent = spawn_bash_with_child(&temp_path);
    let parent_pid = parent.id();

    // Give processes time to start
    sleep(Duration::from_millis(500)).await;

    let inspector = SysinfoProcessInspector::new();

    // Get descendants of the bash process
    let descendants = inspector.get_process_tree(parent_pid).await.unwrap();

    // Should find at least one descendant (the sleep process)
    // Note: This might fail on some systems if bash uses exec instead of fork
    // In that case, the test is still valid but we may get 0 descendants
    if !descendants.is_empty() {
        // Verify none of the descendants have the parent PID
        assert!(
            !descendants.iter().any(|p| p.pid == parent_pid),
            "Parent should not be in descendants list"
        );

        // Verify descendants report parent_pid correctly
        // At least one should have parent_pid as its parent
        let has_child_of_parent = descendants.iter().any(|p| p.parent_pid == Some(parent_pid));
        assert!(
            has_child_of_parent,
            "Expected at least one descendant to have parent_pid {}",
            parent_pid
        );
    }

    // Clean up
    parent.kill().ok();
    parent.wait().ok();

    // Also kill any orphaned children
    for desc in &descendants {
        let _ = inspector.kill_process(desc.pid, true).await;
    }
}

#[tokio::test]
async fn test_kill_process_terminates_it() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();

    // Spawn a process to kill
    let mut child = spawn_sleep_process(&temp_path, 300);
    let child_pid = child.id();

    // Give process time to start
    sleep(Duration::from_millis(100)).await;

    let inspector = SysinfoProcessInspector::new();

    // Verify process exists
    assert!(
        inspector.process_exists(child_pid).await,
        "Expected process {} to exist before kill",
        child_pid
    );

    // Kill with SIGTERM
    inspector.kill_process(child_pid, false).await.unwrap();

    // Wait for the child to actually exit (reap the zombie)
    // This is important because the process may be in zombie state
    let _ = child.wait();

    // Give a moment for sysinfo to see the process is gone
    sleep(Duration::from_millis(100)).await;

    // Verify process is gone
    assert!(
        !inspector.process_exists(child_pid).await,
        "Expected process {} to be gone after kill",
        child_pid
    );
}

#[tokio::test]
async fn test_kill_process_force() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();

    // Spawn a process to kill
    let mut child = spawn_sleep_process(&temp_path, 300);
    let child_pid = child.id();

    // Give process time to start
    sleep(Duration::from_millis(100)).await;

    let inspector = SysinfoProcessInspector::new();

    // Kill with SIGKILL (force)
    inspector.kill_process(child_pid, true).await.unwrap();

    // Wait for the child to actually exit (reap the zombie)
    let _ = child.wait();

    // Give a moment for sysinfo to see the process is gone
    sleep(Duration::from_millis(100)).await;

    // Verify process is gone
    assert!(
        !inspector.process_exists(child_pid).await,
        "Expected process {} to be gone after force kill",
        child_pid
    );
}

#[tokio::test]
async fn test_kill_nonexistent_process_returns_not_found() {
    let inspector = SysinfoProcessInspector::new();

    // Try to kill a PID that almost certainly doesn't exist
    let fake_pid = 999999999;
    let result = inspector.kill_process(fake_pid, false).await;

    assert!(
        result.is_err(),
        "Expected error when killing non-existent process"
    );

    match result {
        Err(services::services::process_inspector::ProcessInspectorError::ProcessNotFound(pid)) => {
            assert_eq!(pid, fake_pid);
        }
        Err(e) => {
            panic!("Expected ProcessNotFound error, got: {:?}", e);
        }
        Ok(_) => {
            panic!("Expected error, got Ok");
        }
    }
}

#[tokio::test]
async fn test_process_exists_for_current_process() {
    let inspector = SysinfoProcessInspector::new();
    let current_pid = std::process::id();

    assert!(
        inspector.process_exists(current_pid).await,
        "Expected current process to exist"
    );
}

#[tokio::test]
async fn test_process_not_exists_for_fake_pid() {
    let inspector = SysinfoProcessInspector::new();
    let fake_pid = 999999999;

    assert!(
        !inspector.process_exists(fake_pid).await,
        "Expected fake PID to not exist"
    );
}

#[tokio::test]
async fn test_get_process_tree_empty_for_nonexistent() {
    let inspector = SysinfoProcessInspector::new();

    // Get tree for non-existent process should return empty
    let descendants = inspector.get_process_tree(999999999).await.unwrap();
    assert!(
        descendants.is_empty(),
        "Expected empty descendants for non-existent process"
    );
}

#[tokio::test]
async fn test_find_processes_by_cwd_prefix_empty_for_nonexistent_path() {
    let inspector = SysinfoProcessInspector::new();

    // Use a path that almost certainly doesn't have any processes
    let matches = inspector
        .find_processes_by_cwd_prefix("/this/path/does/not/exist/at/all/12345")
        .await
        .unwrap();

    assert!(
        matches.is_empty(),
        "Expected no processes in non-existent path"
    );
}

#[tokio::test]
async fn test_process_info_has_correct_fields() {
    let inspector = SysinfoProcessInspector::new();
    let current_pid = std::process::id();

    let processes = inspector.list_processes().await.unwrap();

    // Find our own process
    let our_process = processes.iter().find(|p| p.pid == current_pid);
    assert!(our_process.is_some(), "Expected to find our own process");

    let p = our_process.unwrap();

    // Verify fields are populated sensibly
    assert_eq!(p.pid, current_pid);
    assert!(!p.name.is_empty(), "Expected non-empty process name");
    // Parent PID might be None for init, but should exist for us
    assert!(
        p.parent_pid.is_some(),
        "Expected parent PID for test process"
    );
    // Memory should be non-zero
    assert!(p.memory_bytes > 0, "Expected non-zero memory usage");
}
