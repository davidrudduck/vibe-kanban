//! Handler functions for projects routes.
//!
//! Handlers are organized by concern:
//! - `core`: CRUD operations (list, get, create, update, delete, orphaned, scan, branches, editor)
//! - `merged`: Merged project views combining local and remote
//! - `files`: File browser, search, and file content
//! - `linking`: Remote project linking and members
//! - `github`: GitHub integration (enable, counts, sync)

pub mod core;
pub mod files;
pub mod github;
pub mod linking;
pub mod merged;

// Re-export all handlers for convenient access from the router
pub use core::{
    apply_remote_project_link, create_project, delete_orphaned_projects, delete_project,
    get_project, get_project_branches, get_projects, list_orphaned_projects,
    open_project_in_editor, scan_project_config, truncate_node_name, update_project,
};
pub use files::{
    list_project_files, read_project_file, read_project_file_by_remote_id, search_project_files,
};
pub use github::{get_github_counts, set_github_enabled, sync_github_counts};
pub use linking::{get_project_remote_members, get_remote_project_by_id, link_to_local_folder};
pub use merged::get_merged_projects;
