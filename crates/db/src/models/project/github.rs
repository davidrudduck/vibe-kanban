//! GitHub integration queries for projects.
//!
//! These operations handle GitHub-related functionality like syncing issues/PRs,
//! enabling/disabling GitHub integration, and querying GitHub-enabled projects.

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use super::Project;

impl Project {
    /// Find all projects with GitHub integration enabled
    pub async fn find_github_enabled(pool: &SqlitePool) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Project,
            r#"SELECT id as "id!: Uuid",
                      name,
                      git_repo_path,
                      setup_script,
                      dev_script,
                      cleanup_script,
                      copy_files,
                      parallel_setup_script as "parallel_setup_script!: bool",
                      remote_project_id as "remote_project_id: Uuid",
                      created_at as "created_at!: DateTime<Utc>",
                      updated_at as "updated_at!: DateTime<Utc>",
                      is_remote as "is_remote!: bool",
                      source_node_id as "source_node_id: Uuid",
                      source_node_name,
                      source_node_public_url,
                      source_node_status,
                      remote_last_synced_at as "remote_last_synced_at: DateTime<Utc>",
                      github_enabled as "github_enabled!: bool",
                      github_owner,
                      github_repo,
                      github_open_issues as "github_open_issues!: i32",
                      github_open_prs as "github_open_prs!: i32",
                      github_last_synced_at as "github_last_synced_at: DateTime<Utc>"
               FROM projects
               WHERE github_enabled = 1 AND is_remote = 0
               ORDER BY name"#
        )
        .fetch_all(pool)
        .await
    }

    /// Update GitHub counts for a project
    pub async fn update_github_counts(
        pool: &SqlitePool,
        id: Uuid,
        open_issues: i32,
        open_prs: i32,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now();
        sqlx::query!(
            r#"UPDATE projects
               SET github_open_issues = $2,
                   github_open_prs = $3,
                   github_last_synced_at = $4
               WHERE id = $1"#,
            id,
            open_issues,
            open_prs,
            now
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Enable or disable GitHub integration for a project
    pub async fn set_github_enabled(
        pool: &SqlitePool,
        id: Uuid,
        enabled: bool,
        owner: Option<String>,
        repo: Option<String>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"UPDATE projects
               SET github_enabled = $2,
                   github_owner = $3,
                   github_repo = $4
               WHERE id = $1"#,
            id,
            enabled,
            owner,
            repo
        )
        .execute(pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::project::CreateProject;
    use crate::test_utils::create_test_pool;

    #[tokio::test]
    async fn test_find_github_enabled_empty() {
        let (pool, _temp_dir) = create_test_pool().await;

        let projects = Project::find_github_enabled(&pool).await.unwrap();
        assert!(projects.is_empty());
    }

    #[tokio::test]
    async fn test_set_github_enabled() {
        let (pool, _temp_dir) = create_test_pool().await;

        let project_id = Uuid::new_v4();
        let create_data = CreateProject {
            name: "GitHub Test".to_string(),
            git_repo_path: "/github/test".to_string(),
            use_existing_repo: true,
            clone_url: None,
            setup_script: None,
            dev_script: None,
            cleanup_script: None,
            copy_files: None,
        };

        Project::create(&pool, &create_data, project_id).await.unwrap();

        // Initially not GitHub enabled
        let projects = Project::find_github_enabled(&pool).await.unwrap();
        assert!(projects.is_empty());

        // Enable GitHub
        Project::set_github_enabled(
            &pool,
            project_id,
            true,
            Some("anthropics".to_string()),
            Some("claude-code".to_string()),
        )
        .await
        .unwrap();

        let projects = Project::find_github_enabled(&pool).await.unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].github_owner, Some("anthropics".to_string()));
        assert_eq!(projects[0].github_repo, Some("claude-code".to_string()));
    }

    #[tokio::test]
    async fn test_update_github_counts() {
        let (pool, _temp_dir) = create_test_pool().await;

        let project_id = Uuid::new_v4();
        let create_data = CreateProject {
            name: "Counts Test".to_string(),
            git_repo_path: "/counts/test".to_string(),
            use_existing_repo: true,
            clone_url: None,
            setup_script: None,
            dev_script: None,
            cleanup_script: None,
            copy_files: None,
        };

        Project::create(&pool, &create_data, project_id).await.unwrap();

        // Initially counts are 0
        let project = Project::find_by_id(&pool, project_id).await.unwrap().unwrap();
        assert_eq!(project.github_open_issues, 0);
        assert_eq!(project.github_open_prs, 0);

        // Update counts
        Project::update_github_counts(&pool, project_id, 10, 5)
            .await
            .unwrap();

        let project = Project::find_by_id(&pool, project_id).await.unwrap().unwrap();
        assert_eq!(project.github_open_issues, 10);
        assert_eq!(project.github_open_prs, 5);
        assert!(project.github_last_synced_at.is_some());
    }

    #[tokio::test]
    async fn test_disable_github() {
        let (pool, _temp_dir) = create_test_pool().await;

        let project_id = Uuid::new_v4();
        let create_data = CreateProject {
            name: "Disable Test".to_string(),
            git_repo_path: "/disable/test".to_string(),
            use_existing_repo: true,
            clone_url: None,
            setup_script: None,
            dev_script: None,
            cleanup_script: None,
            copy_files: None,
        };

        Project::create(&pool, &create_data, project_id).await.unwrap();

        // Enable GitHub
        Project::set_github_enabled(
            &pool,
            project_id,
            true,
            Some("owner".to_string()),
            Some("repo".to_string()),
        )
        .await
        .unwrap();

        let projects = Project::find_github_enabled(&pool).await.unwrap();
        assert_eq!(projects.len(), 1);

        // Disable GitHub
        Project::set_github_enabled(&pool, project_id, false, None, None)
            .await
            .unwrap();

        let projects = Project::find_github_enabled(&pool).await.unwrap();
        assert!(projects.is_empty());
    }
}
