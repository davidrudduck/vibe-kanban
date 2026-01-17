use services::services::filesystem::FilesystemService;
use std::fs;
use tempfile::TempDir;

#[tokio::test]
async fn test_list_directory_includes_dot_files() {
    // Create temporary directory structure
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let temp_path = temp_dir.path();

    // Create test files and directories
    fs::write(temp_path.join(".env"), "SECRET=test").expect("Failed to create .env");
    fs::write(temp_path.join(".gitignore"), "node_modules/").expect("Failed to create .gitignore");
    fs::create_dir(temp_path.join(".hidden_dir")).expect("Failed to create .hidden_dir");
    fs::write(temp_path.join("visible.txt"), "Hello").expect("Failed to create visible.txt");

    // Test the filesystem service
    let service = FilesystemService::new();
    let result = service
        .list_directory(Some(temp_path.to_string_lossy().to_string()))
        .await
        .expect("Failed to list directory");

    // Extract entry names for easier assertion
    let entry_names: Vec<&str> = result.entries.iter().map(|e| e.name.as_str()).collect();

    // Assert all files are present (including dot files)
    assert_eq!(
        entry_names.len(),
        4,
        "Expected 4 entries, got {}: {:?}",
        entry_names.len(),
        entry_names
    );
    assert!(entry_names.contains(&".env"), "Missing .env file");
    assert!(
        entry_names.contains(&".gitignore"),
        "Missing .gitignore file"
    );
    assert!(
        entry_names.contains(&".hidden_dir"),
        "Missing .hidden_dir directory"
    );
    assert!(
        entry_names.contains(&"visible.txt"),
        "Missing visible.txt file"
    );
}

#[tokio::test]
async fn test_list_directory_within_includes_dot_files() {
    // RED: This test will fail because dot files are currently filtered
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let temp_path = temp_dir.path();

    // Create nested structure with dot directory
    fs::create_dir(temp_path.join(".claude")).expect("Failed to create .claude directory");
    fs::write(temp_path.join(".claude/plan.md"), "# Plan").expect("Failed to create plan.md");
    fs::write(temp_path.join(".claude/notes.txt"), "notes").expect("Failed to create notes.txt");

    let service = FilesystemService::new();

    // List root - should show .claude directory
    let root_result = service
        .list_directory_within(temp_path, None)
        .await
        .expect("Failed to list root directory");

    let root_names: Vec<&str> = root_result
        .entries
        .iter()
        .map(|e| e.name.as_str())
        .collect();

    assert!(
        root_names.contains(&".claude"),
        "Should show .claude directory in root listing, got: {:?}",
        root_names
    );

    // List inside .claude - should show files
    let claude_result = service
        .list_directory_within(temp_path, Some(".claude"))
        .await
        .expect("Failed to list .claude directory");

    let claude_names: Vec<&str> = claude_result
        .entries
        .iter()
        .map(|e| e.name.as_str())
        .collect();

    assert!(
        claude_names.contains(&"plan.md"),
        "Should show plan.md inside .claude directory, got: {:?}",
        claude_names
    );
    assert!(
        claude_names.contains(&"notes.txt"),
        "Should show notes.txt inside .claude directory, got: {:?}",
        claude_names
    );
}

#[tokio::test]
async fn test_path_traversal_blocked_with_dot_files() {
    // GREEN from start: Security should still work even with dot files visible
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let temp_path = temp_dir.path();

    // Create a directory structure with dot files
    fs::create_dir(temp_path.join("allowed")).expect("Failed to create allowed directory");
    fs::write(temp_path.join("allowed/.env"), "SECRET=value").expect("Failed to create .env");

    let service = FilesystemService::new();

    // Attempt path traversal using ../.. - should be blocked
    let result = service
        .list_directory_within(&temp_path.join("allowed"), Some("../.."))
        .await;

    // Assert that the operation is rejected
    assert!(
        result.is_err(),
        "Path traversal should be blocked even with dot files visible"
    );
}

#[tokio::test]
async fn test_parent_directory_navigation_still_works() {
    // GREEN from start: Parent directory navigation (..) should work correctly
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let temp_path = temp_dir.path();

    // Create structure with subdirectory and root file
    fs::create_dir(temp_path.join("subdir")).expect("Failed to create subdir");
    fs::write(temp_path.join("root.txt"), "root file").expect("Failed to create root.txt");
    fs::write(temp_path.join("subdir/nested.txt"), "nested file")
        .expect("Failed to create nested.txt");

    let service = FilesystemService::new();

    // Navigate into subdirectory and verify path
    let subdir_result = service
        .list_directory_within(temp_path, Some("subdir"))
        .await
        .expect("Failed to navigate into subdirectory");

    // Verify we're in the subdirectory
    assert!(
        subdir_result.current_path.ends_with("subdir"),
        "Expected path to end with 'subdir', got: {}",
        subdir_result.current_path
    );

    // Verify we can see the nested file
    let entry_names: Vec<&str> = subdir_result
        .entries
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert!(
        entry_names.contains(&"nested.txt"),
        "Should see nested.txt in subdir, got: {:?}",
        entry_names
    );
}
