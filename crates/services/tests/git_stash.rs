//! Tests for git stash operations
//!
//! These tests verify that the stash_changes() and pop_stash() functions
//! work correctly for the interactive stash dialog workflow.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use git2::Repository;
use services::services::git::{GitCli, GitService, GitServiceError};
use tempfile::TempDir;

fn write_file<P: AsRef<Path>>(base: P, rel: &str, content: &str) {
    let path = base.as_ref().join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let mut f = fs::File::create(&path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

fn configure_user(repo_path: &Path, name: &str, email: &str) {
    let repo = Repository::open(repo_path).unwrap();
    let mut cfg = repo.config().unwrap();
    cfg.set_str("user.name", name).unwrap();
    cfg.set_str("user.email", email).unwrap();
}

fn init_repo_main(root: &TempDir) -> PathBuf {
    let path = root.path().join("repo");
    let s = GitService::new();
    s.initialize_repo_with_main_branch(&path).unwrap();
    configure_user(&path, "Test User", "test@example.com");
    path
}

/// Test 1: stash_changes creates a stash entry
#[test]
fn test_stash_changes_creates_entry() {
    let td = TempDir::new().unwrap();
    let repo_path = init_repo_main(&td);
    let s = GitService::new();

    // Add an initial tracked file and commit it
    write_file(&repo_path, "tracked.txt", "initial content\n");
    s.commit(&repo_path, "add tracked file").unwrap();

    // Modify the tracked file (creating uncommitted changes)
    write_file(&repo_path, "tracked.txt", "modified content\n");

    // Verify we have uncommitted changes
    assert!(!s.is_worktree_clean(&repo_path).unwrap());

    // Stash the changes
    let stash_ref = s.stash_changes(&repo_path, None).unwrap();
    assert!(!stash_ref.is_empty(), "stash ref should not be empty");

    // Verify worktree is now clean
    assert!(s.is_worktree_clean(&repo_path).unwrap());

    // Verify file content is back to original
    let content = fs::read_to_string(repo_path.join("tracked.txt")).unwrap();
    assert_eq!(content, "initial content\n");
}

/// Test 2: pop_stash restores changes
#[test]
fn test_pop_stash_restores_changes() {
    let td = TempDir::new().unwrap();
    let repo_path = init_repo_main(&td);
    let s = GitService::new();

    // Add an initial tracked file and commit it
    write_file(&repo_path, "tracked.txt", "initial content\n");
    s.commit(&repo_path, "add tracked file").unwrap();

    // Modify the tracked file
    write_file(&repo_path, "tracked.txt", "modified content\n");

    // Stash the changes
    let _stash_ref = s.stash_changes(&repo_path, None).unwrap();
    assert!(s.is_worktree_clean(&repo_path).unwrap());

    // Pop the stash
    s.pop_stash(&repo_path).unwrap();

    // Verify changes are restored
    assert!(!s.is_worktree_clean(&repo_path).unwrap());
    let content = fs::read_to_string(repo_path.join("tracked.txt")).unwrap();
    assert_eq!(content, "modified content\n");
}

/// Test 3: stash with message includes message
#[test]
fn test_stash_with_custom_message() {
    let td = TempDir::new().unwrap();
    let repo_path = init_repo_main(&td);
    let s = GitService::new();

    // Add an initial tracked file and commit it
    write_file(&repo_path, "tracked.txt", "initial content\n");
    s.commit(&repo_path, "add tracked file").unwrap();

    // Modify the tracked file
    write_file(&repo_path, "tracked.txt", "modified content\n");

    // Stash the changes with a custom message
    let stash_ref = s
        .stash_changes(&repo_path, Some("WIP: feature work"))
        .unwrap();
    assert!(!stash_ref.is_empty());

    // Verify the stash message contains our custom message
    let git = GitCli::new();
    let stash_list = git.git(&repo_path, ["stash", "list"]).unwrap();
    assert!(
        stash_list.contains("WIP: feature work"),
        "stash list should contain custom message"
    );
}

/// Test 4: pop_stash on empty stash returns error
#[test]
fn test_pop_empty_stash_returns_error() {
    let td = TempDir::new().unwrap();
    let repo_path = init_repo_main(&td);
    let s = GitService::new();

    // Try to pop from an empty stash
    let result = s.pop_stash(&repo_path);

    // Should return an error
    assert!(result.is_err());
    let err = result.unwrap_err();
    match err {
        GitServiceError::StashEmpty => {}
        other => panic!("expected StashEmpty error, got: {:?}", other),
    }
}

/// Test 5: stash on clean worktree returns error
#[test]
fn test_stash_clean_worktree_returns_error() {
    let td = TempDir::new().unwrap();
    let repo_path = init_repo_main(&td);
    let s = GitService::new();

    // Try to stash when there are no changes
    let result = s.stash_changes(&repo_path, None);

    // Should return an error
    assert!(result.is_err());
    let err = result.unwrap_err();
    match err {
        GitServiceError::NothingToStash => {}
        other => panic!("expected NothingToStash error, got: {:?}", other),
    }
}

/// Test 6: stash includes untracked files when requested
#[test]
fn test_stash_includes_untracked_files() {
    let td = TempDir::new().unwrap();
    let repo_path = init_repo_main(&td);
    let s = GitService::new();

    // Add an initial tracked file and commit it
    write_file(&repo_path, "tracked.txt", "tracked\n");
    s.commit(&repo_path, "add tracked file").unwrap();

    // Create an untracked file
    write_file(&repo_path, "untracked.txt", "untracked content\n");

    // Also modify the tracked file to ensure there's something to stash
    write_file(&repo_path, "tracked.txt", "tracked modified\n");

    // Stash all changes including untracked
    let _stash_ref = s.stash_changes(&repo_path, None).unwrap();

    // Verify untracked file is gone (stashed)
    assert!(!repo_path.join("untracked.txt").exists());

    // Pop and verify untracked file is back
    s.pop_stash(&repo_path).unwrap();
    assert!(repo_path.join("untracked.txt").exists());
    let content = fs::read_to_string(repo_path.join("untracked.txt")).unwrap();
    assert_eq!(content, "untracked content\n");
}

/// Test 7: get_dirty_files returns list of modified files
#[test]
fn test_get_dirty_files_returns_modified_files() {
    let td = TempDir::new().unwrap();
    let repo_path = init_repo_main(&td);
    let s = GitService::new();

    // Add initial tracked files and commit
    write_file(&repo_path, "file1.txt", "content1\n");
    write_file(&repo_path, "file2.txt", "content2\n");
    s.commit(&repo_path, "add files").unwrap();

    // Modify one file
    write_file(&repo_path, "file1.txt", "modified1\n");

    // Get dirty files
    let dirty_files = s.get_dirty_files(&repo_path).unwrap();

    assert_eq!(dirty_files.len(), 1);
    assert!(dirty_files.iter().any(|f| f.contains("file1.txt")));
}

/// Test 8: get_dirty_files returns empty vec when clean
#[test]
fn test_get_dirty_files_empty_when_clean() {
    let td = TempDir::new().unwrap();
    let repo_path = init_repo_main(&td);
    let s = GitService::new();

    // Add initial tracked file and commit
    write_file(&repo_path, "file1.txt", "content1\n");
    s.commit(&repo_path, "add file").unwrap();

    // Get dirty files (should be empty)
    let dirty_files = s.get_dirty_files(&repo_path).unwrap();
    assert!(dirty_files.is_empty());
}
