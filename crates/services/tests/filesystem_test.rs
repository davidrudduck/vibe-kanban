use services::services::filesystem::FilesystemService;
use std::fs;
use tempfile::TempDir;

#[tokio::test]
async fn test_list_directory_includes_dot_files() {
    // RED: This test will fail because dot files are currently filtered
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    // Create test structure with dot files
    fs::create_dir(temp_path.join(".hidden_dir")).unwrap();
    fs::write(temp_path.join(".env"), "SECRET=value").unwrap();
    fs::write(temp_path.join(".gitignore"), "*.log").unwrap();
    fs::write(temp_path.join("visible.txt"), "content").unwrap();

    // List directory
    let fs_service = FilesystemService::new();
    let result = fs_service
        .list_directory(Some(temp_path.to_string_lossy().to_string()))
        .await
        .unwrap();

    // Verify dot files are included
    let names: Vec<&str> = result.entries.iter().map(|e| e.name.as_str()).collect();

    assert!(names.contains(&".hidden_dir"), "Should include .hidden_dir");
    assert!(names.contains(&".env"), "Should include .env");
    assert!(names.contains(&".gitignore"), "Should include .gitignore");
    assert!(names.contains(&"visible.txt"), "Should include visible.txt");
    assert_eq!(names.len(), 4, "Should have exactly 4 entries");
}

#[tokio::test]
async fn test_list_directory_within_includes_dot_files() {
    // RED: This test will fail because dot files are currently filtered
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    // Create nested structure with dot directory
    fs::create_dir(temp_path.join(".claude")).unwrap();
    fs::write(temp_path.join(".claude/plan.md"), "# Plan").unwrap();
    fs::write(temp_path.join(".claude/notes.txt"), "notes").unwrap();

    let fs_service = FilesystemService::new();

    // List root - should show .claude directory
    let root_result = fs_service
        .list_directory_within(temp_path, None)
        .await
        .unwrap();
    let root_names: Vec<&str> = root_result
        .entries
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert!(
        root_names.contains(&".claude"),
        "Should show .claude directory"
    );

    // List inside .claude - should show files
    let claude_result = fs_service
        .list_directory_within(temp_path, Some(".claude"))
        .await
        .unwrap();
    let claude_names: Vec<&str> = claude_result
        .entries
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert!(claude_names.contains(&"plan.md"), "Should show plan.md");
    assert!(claude_names.contains(&"notes.txt"), "Should show notes.txt");
}

#[tokio::test]
async fn test_path_traversal_blocked_with_dot_files() {
    // GREEN from start: Security should still work
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    fs::create_dir(temp_path.join("allowed")).unwrap();
    fs::write(temp_path.join("allowed/.env"), "SECRET=value").unwrap();

    let fs_service = FilesystemService::new();

    // Attempt path traversal - should fail
    let result = fs_service
        .list_directory_within(&temp_path.join("allowed"), Some("../.."))
        .await;

    assert!(result.is_err(), "Path traversal should be blocked");
}
