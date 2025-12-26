//! Tests for git clone operations
//!
//! These tests verify that the clone_repository() function works correctly
//! for the "Clone from URL" workflow in project creation.

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

/// Test 1: clone_repository clones a local repo to specified path
#[test]
fn test_clone_repository_creates_repo() {
    let td = TempDir::new().unwrap();

    // Create a source repo with some content
    let source_path = init_repo_main(&td);
    write_file(&source_path, "README.md", "# Test Repo\n");
    let s = GitService::new();
    s.commit(&source_path, "add readme").unwrap();

    // Clone to a new destination
    let dest_path = td.path().join("cloned");

    // Use file:// URL for local clone
    let clone_url = format!("file://{}", source_path.display());
    s.clone_repo(&clone_url, &dest_path).unwrap();

    // Verify destination is a valid git repo
    assert!(dest_path.join(".git").exists());

    // Verify content was cloned
    let readme_content = fs::read_to_string(dest_path.join("README.md")).unwrap();
    assert_eq!(readme_content, "# Test Repo\n");
}

/// Test 2: clone to non-empty directory fails
#[test]
fn test_clone_to_nonempty_dir_fails() {
    let td = TempDir::new().unwrap();

    // Create a source repo
    let source_path = init_repo_main(&td);
    write_file(&source_path, "README.md", "# Test\n");
    let s = GitService::new();
    s.commit(&source_path, "add readme").unwrap();

    // Create destination with existing files
    let dest_path = td.path().join("nonempty");
    fs::create_dir_all(&dest_path).unwrap();
    write_file(&dest_path, "existing.txt", "I was here first\n");

    // Try to clone
    let clone_url = format!("file://{}", source_path.display());
    let result = s.clone_repo(&clone_url, &dest_path);

    // Should fail with DestinationNotEmpty error
    assert!(result.is_err());
    let err = result.unwrap_err();
    match err {
        GitServiceError::DestinationNotEmpty(_) => {}
        other => panic!("expected DestinationNotEmpty error, got: {:?}", other),
    }
}

/// Test 3: clone with invalid URL fails
#[test]
fn test_clone_invalid_url_fails() {
    let td = TempDir::new().unwrap();
    let dest_path = td.path().join("dest");

    let s = GitService::new();

    // Try to clone from an invalid URL
    let result = s.clone_repo("not-a-valid-url", &dest_path);

    // Should fail with InvalidUrl or CloneFailed error
    assert!(result.is_err());
    // The error type depends on git's error message, just check it fails
}

/// Test 4: clone non-existent repository fails gracefully
#[test]
fn test_clone_nonexistent_repo_fails() {
    let td = TempDir::new().unwrap();
    let dest_path = td.path().join("dest");

    let s = GitService::new();

    // Try to clone from a non-existent local path
    let fake_path = td.path().join("does-not-exist");
    let clone_url = format!("file://{}", fake_path.display());
    let result = s.clone_repo(&clone_url, &dest_path);

    // Should fail with an error
    assert!(result.is_err());
    let err = result.unwrap_err();
    match err {
        GitServiceError::CloneFailed(_) => {}
        other => panic!("expected CloneFailed error, got: {:?}", other),
    }
}

/// Test 5: clone to existing empty directory succeeds
#[test]
fn test_clone_to_empty_dir_succeeds() {
    let td = TempDir::new().unwrap();

    // Create a source repo
    let source_path = init_repo_main(&td);
    write_file(&source_path, "file.txt", "content\n");
    let s = GitService::new();
    s.commit(&source_path, "add file").unwrap();

    // Create empty destination directory
    let dest_path = td.path().join("empty-dest");
    fs::create_dir_all(&dest_path).unwrap();

    // Clone should succeed
    let clone_url = format!("file://{}", source_path.display());
    s.clone_repo(&clone_url, &dest_path).unwrap();

    // Verify repo was created
    assert!(dest_path.join(".git").exists());
    assert!(dest_path.join("file.txt").exists());
}

/// Test 6: clone preserves branch history
#[test]
fn test_clone_preserves_history() {
    let td = TempDir::new().unwrap();

    // Create a source repo with multiple commits
    let source_path = init_repo_main(&td);
    let s = GitService::new();

    write_file(&source_path, "file1.txt", "first\n");
    s.commit(&source_path, "first commit").unwrap();

    write_file(&source_path, "file2.txt", "second\n");
    s.commit(&source_path, "second commit").unwrap();

    // Clone
    let dest_path = td.path().join("cloned");
    let clone_url = format!("file://{}", source_path.display());
    s.clone_repo(&clone_url, &dest_path).unwrap();

    // Verify we can read history
    let git = GitCli::new();
    let log = git.git(&dest_path, ["log", "--oneline"]).unwrap();

    // Should have at least the commits we made (plus initial)
    let commit_count = log.lines().count();
    assert!(
        commit_count >= 3,
        "expected at least 3 commits, got: {}",
        commit_count
    );
}
