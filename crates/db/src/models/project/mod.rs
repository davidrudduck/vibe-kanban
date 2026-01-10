//! Project model for managing projects in Vibe Kanban.
//!
//! A project represents a git repository with associated tasks, configuration,
//! and optional GitHub integration. Projects can be local (created on this node)
//! or remote (synced from the Hive).

mod github;
mod queries;
mod stats;
mod sync;

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use thiserror::Error;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("Project not found")]
    ProjectNotFound,
    #[error("Project with git repository path already exists")]
    GitRepoPathExists,
    #[error("Failed to check existing git repository path: {0}")]
    GitRepoCheckFailed(String),
    #[error("Failed to create project: {0}")]
    CreateFailed(String),
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub git_repo_path: PathBuf,
    pub setup_script: Option<String>,
    pub dev_script: Option<String>,
    pub cleanup_script: Option<String>,
    pub copy_files: Option<String>,
    /// When true, setup script runs concurrently with the coding agent
    pub parallel_setup_script: bool,
    pub remote_project_id: Option<Uuid>,
    #[ts(type = "Date")]
    pub created_at: DateTime<Utc>,
    #[ts(type = "Date")]
    pub updated_at: DateTime<Utc>,
    // Remote project fields (Phase 1F)
    pub is_remote: bool,
    pub source_node_id: Option<Uuid>,
    pub source_node_name: Option<String>,
    pub source_node_public_url: Option<String>,
    pub source_node_status: Option<String>,
    #[ts(type = "Date | null")]
    pub remote_last_synced_at: Option<DateTime<Utc>>,
    // GitHub integration fields
    /// Whether GitHub integration is enabled for this project
    pub github_enabled: bool,
    /// GitHub repository owner (e.g., "anthropics" from "anthropics/claude-code")
    pub github_owner: Option<String>,
    /// GitHub repository name (e.g., "claude-code" from "anthropics/claude-code")
    pub github_repo: Option<String>,
    /// Count of open issues (cached from GitHub API)
    pub github_open_issues: i32,
    /// Count of open pull requests (cached from GitHub API)
    pub github_open_prs: i32,
    /// Timestamp of last successful sync with GitHub API
    #[ts(type = "Date | null")]
    pub github_last_synced_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, TS)]
pub struct CreateProject {
    pub name: String,
    pub git_repo_path: String,
    pub use_existing_repo: bool,
    /// URL to clone repository from (mutually exclusive with use_existing_repo=true)
    pub clone_url: Option<String>,
    pub setup_script: Option<String>,
    pub dev_script: Option<String>,
    pub cleanup_script: Option<String>,
    pub copy_files: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
pub struct UpdateProject {
    pub name: Option<String>,
    pub git_repo_path: Option<String>,
    pub setup_script: Option<String>,
    pub dev_script: Option<String>,
    pub cleanup_script: Option<String>,
    pub copy_files: Option<String>,
    pub parallel_setup_script: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct SearchResult {
    pub path: String,
    pub is_file: bool,
    pub match_type: SearchMatchType,
}

/// Task counts for a project (used in merged projects view)
#[derive(Debug, Clone, Default)]
pub struct ProjectTaskCounts {
    pub todo: i32,
    pub in_progress: i32,
    pub in_review: i32,
    pub done: i32,
}

/// Data returned for local projects with last attempt and task counts
pub struct LocalProjectWithStats {
    pub project: Project,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub task_counts: ProjectTaskCounts,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub enum SearchMatchType {
    FileName,
    DirectoryName,
    FullPath,
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct ProjectConfigSuggestion {
    pub field: ProjectConfigField,
    pub value: String,
    pub confidence: ConfidenceLevel,
    pub source: String,
}

#[derive(Debug, Serialize, Deserialize, TS, Clone, PartialEq)]
#[ts(export)]
pub enum ProjectConfigField {
    SetupScript,
    DevScript,
    CleanupScript,
    CopyFiles,
    DevHost,
    DevPort,
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub enum ConfidenceLevel {
    High,
    Medium,
}

#[derive(Debug, Deserialize, TS)]
pub struct ScanConfigRequest {
    pub repo_path: String,
}

#[derive(Debug, Serialize, TS)]
pub struct ScanConfigResponse {
    pub suggestions: Vec<ProjectConfigSuggestion>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_task_counts_default() {
        let counts = ProjectTaskCounts::default();
        assert_eq!(counts.todo, 0);
        assert_eq!(counts.in_progress, 0);
        assert_eq!(counts.in_review, 0);
        assert_eq!(counts.done, 0);
    }

    #[test]
    fn test_project_error_display() {
        let err = ProjectError::ProjectNotFound;
        assert_eq!(format!("{}", err), "Project not found");

        let err = ProjectError::GitRepoPathExists;
        assert_eq!(
            format!("{}", err),
            "Project with git repository path already exists"
        );

        let err = ProjectError::GitRepoCheckFailed("test error".to_string());
        assert_eq!(
            format!("{}", err),
            "Failed to check existing git repository path: test error"
        );

        let err = ProjectError::CreateFailed("creation error".to_string());
        assert_eq!(
            format!("{}", err),
            "Failed to create project: creation error"
        );
    }

    #[test]
    fn test_search_match_type_serialization() {
        let match_type = SearchMatchType::FileName;
        let json = serde_json::to_string(&match_type).unwrap();
        assert_eq!(json, "\"FileName\"");

        let match_type = SearchMatchType::DirectoryName;
        let json = serde_json::to_string(&match_type).unwrap();
        assert_eq!(json, "\"DirectoryName\"");

        let match_type = SearchMatchType::FullPath;
        let json = serde_json::to_string(&match_type).unwrap();
        assert_eq!(json, "\"FullPath\"");
    }

    #[test]
    fn test_confidence_level_serialization() {
        let level = ConfidenceLevel::High;
        let json = serde_json::to_string(&level).unwrap();
        assert_eq!(json, "\"High\"");

        let level = ConfidenceLevel::Medium;
        let json = serde_json::to_string(&level).unwrap();
        assert_eq!(json, "\"Medium\"");
    }

    #[test]
    fn test_project_config_field_equality() {
        assert_eq!(
            ProjectConfigField::SetupScript,
            ProjectConfigField::SetupScript
        );
        assert_ne!(
            ProjectConfigField::SetupScript,
            ProjectConfigField::DevScript
        );
    }
}
