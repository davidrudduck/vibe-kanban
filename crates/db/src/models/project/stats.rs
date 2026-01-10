//! Statistics queries for projects.
//!
//! These operations handle aggregated statistics like task counts,
//! last attempt timestamps, and project activity metrics.

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use super::{LocalProjectWithStats, Project, ProjectTaskCounts};

impl Project {
    /// Find all local projects with their last attempt timestamp for sorting.
    /// Returns tuples of (Project, Option<last_attempt_at>).
    pub async fn find_local_projects_with_last_attempt(
        pool: &SqlitePool,
    ) -> Result<Vec<(Self, Option<DateTime<Utc>>)>, sqlx::Error> {
        // Use a raw query since we need to join and return extra data
        // Note: GROUP BY makes all columns potentially nullable in SQLite/SQLx,
        // so we use "!" annotations to assert non-nullability for required fields
        let rows = sqlx::query!(
            r#"
            SELECT
                p.id as "id!: Uuid",
                p.name as "name!",
                p.git_repo_path as "git_repo_path!",
                p.setup_script,
                p.dev_script,
                p.cleanup_script,
                p.copy_files,
                p.parallel_setup_script as "parallel_setup_script!: bool",
                p.remote_project_id as "remote_project_id: Uuid",
                p.created_at as "created_at!: DateTime<Utc>",
                p.updated_at as "updated_at!: DateTime<Utc>",
                p.is_remote as "is_remote!: bool",
                p.source_node_id as "source_node_id: Uuid",
                p.source_node_name,
                p.source_node_public_url,
                p.source_node_status,
                p.remote_last_synced_at as "remote_last_synced_at: DateTime<Utc>",
                p.github_enabled as "github_enabled!: bool",
                p.github_owner,
                p.github_repo,
                p.github_open_issues as "github_open_issues!: i32",
                p.github_open_prs as "github_open_prs!: i32",
                p.github_last_synced_at as "github_last_synced_at: DateTime<Utc>",
                MAX(ta.updated_at) as "last_attempt_at: DateTime<Utc>"
            FROM projects p
            LEFT JOIN tasks t ON t.project_id = p.id
            LEFT JOIN task_attempts ta ON ta.task_id = t.id
            WHERE p.is_remote = 0
            GROUP BY p.id
            ORDER BY p.created_at DESC
            "#
        )
        .fetch_all(pool)
        .await?;

        let results = rows
            .into_iter()
            .map(|row| {
                let project = Project {
                    id: row.id,
                    name: row.name,
                    git_repo_path: row.git_repo_path.into(),
                    setup_script: row.setup_script,
                    dev_script: row.dev_script,
                    cleanup_script: row.cleanup_script,
                    copy_files: row.copy_files,
                    parallel_setup_script: row.parallel_setup_script,
                    remote_project_id: row.remote_project_id,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                    is_remote: row.is_remote,
                    source_node_id: row.source_node_id,
                    source_node_name: row.source_node_name,
                    source_node_public_url: row.source_node_public_url,
                    source_node_status: row.source_node_status,
                    remote_last_synced_at: row.remote_last_synced_at,
                    github_enabled: row.github_enabled,
                    github_owner: row.github_owner,
                    github_repo: row.github_repo,
                    github_open_issues: row.github_open_issues,
                    github_open_prs: row.github_open_prs,
                    github_last_synced_at: row.github_last_synced_at,
                };
                (project, row.last_attempt_at)
            })
            .collect();

        Ok(results)
    }

    /// Find all local projects with their last attempt timestamp and task counts.
    /// Returns LocalProjectWithStats including task status breakdown.
    pub async fn find_local_projects_with_stats(
        pool: &SqlitePool,
    ) -> Result<Vec<LocalProjectWithStats>, sqlx::Error> {
        // Use a raw query to get project info, last attempt, and task counts
        let rows = sqlx::query!(
            r#"
            SELECT
                p.id as "id!: Uuid",
                p.name as "name!",
                p.git_repo_path as "git_repo_path!",
                p.setup_script,
                p.dev_script,
                p.cleanup_script,
                p.copy_files,
                p.parallel_setup_script as "parallel_setup_script!: bool",
                p.remote_project_id as "remote_project_id: Uuid",
                p.created_at as "created_at!: DateTime<Utc>",
                p.updated_at as "updated_at!: DateTime<Utc>",
                p.is_remote as "is_remote!: bool",
                p.source_node_id as "source_node_id: Uuid",
                p.source_node_name,
                p.source_node_public_url,
                p.source_node_status,
                p.remote_last_synced_at as "remote_last_synced_at: DateTime<Utc>",
                p.github_enabled as "github_enabled!: bool",
                p.github_owner,
                p.github_repo,
                p.github_open_issues as "github_open_issues!: i32",
                p.github_open_prs as "github_open_prs!: i32",
                p.github_last_synced_at as "github_last_synced_at: DateTime<Utc>",
                MAX(ta.updated_at) as "last_attempt_at: DateTime<Utc>",
                COALESCE(SUM(CASE WHEN t.status = 'todo' THEN 1 ELSE 0 END), 0) as "todo_count!: i32",
                COALESCE(SUM(CASE WHEN t.status = 'inprogress' THEN 1 ELSE 0 END), 0) as "in_progress_count!: i32",
                COALESCE(SUM(CASE WHEN t.status = 'inreview' THEN 1 ELSE 0 END), 0) as "in_review_count!: i32",
                COALESCE(SUM(CASE WHEN t.status = 'done' THEN 1 ELSE 0 END), 0) as "done_count!: i32"
            FROM projects p
            LEFT JOIN tasks t ON t.project_id = p.id
            LEFT JOIN task_attempts ta ON ta.task_id = t.id
            WHERE p.is_remote = 0
            GROUP BY p.id
            ORDER BY p.created_at DESC
            "#
        )
        .fetch_all(pool)
        .await?;

        let results = rows
            .into_iter()
            .map(|row| {
                let project = Project {
                    id: row.id,
                    name: row.name,
                    git_repo_path: row.git_repo_path.into(),
                    setup_script: row.setup_script,
                    dev_script: row.dev_script,
                    cleanup_script: row.cleanup_script,
                    copy_files: row.copy_files,
                    parallel_setup_script: row.parallel_setup_script,
                    remote_project_id: row.remote_project_id,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                    is_remote: row.is_remote,
                    source_node_id: row.source_node_id,
                    source_node_name: row.source_node_name,
                    source_node_public_url: row.source_node_public_url,
                    source_node_status: row.source_node_status,
                    remote_last_synced_at: row.remote_last_synced_at,
                    github_enabled: row.github_enabled,
                    github_owner: row.github_owner,
                    github_repo: row.github_repo,
                    github_open_issues: row.github_open_issues,
                    github_open_prs: row.github_open_prs,
                    github_last_synced_at: row.github_last_synced_at,
                };
                LocalProjectWithStats {
                    project,
                    last_attempt_at: row.last_attempt_at,
                    task_counts: ProjectTaskCounts {
                        todo: row.todo_count,
                        in_progress: row.in_progress_count,
                        in_review: row.in_review_count,
                        done: row.done_count,
                    },
                }
            })
            .collect();

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::project::CreateProject;
    use crate::test_utils::create_test_pool;

    #[tokio::test]
    async fn test_find_local_projects_with_last_attempt_empty() {
        let (pool, _temp_dir) = create_test_pool().await;

        let projects = Project::find_local_projects_with_last_attempt(&pool)
            .await
            .unwrap();
        assert!(projects.is_empty());
    }

    #[tokio::test]
    async fn test_find_local_projects_with_last_attempt() {
        let (pool, _temp_dir) = create_test_pool().await;

        let project_id = Uuid::new_v4();
        let create_data = CreateProject {
            name: "Stats Test".to_string(),
            git_repo_path: "/stats/test".to_string(),
            use_existing_repo: true,
            clone_url: None,
            setup_script: None,
            dev_script: None,
            cleanup_script: None,
            copy_files: None,
        };

        Project::create(&pool, &create_data, project_id).await.unwrap();

        let projects = Project::find_local_projects_with_last_attempt(&pool)
            .await
            .unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].0.id, project_id);
        assert!(projects[0].1.is_none()); // No attempts yet
    }

    #[tokio::test]
    async fn test_find_local_projects_with_stats_empty() {
        let (pool, _temp_dir) = create_test_pool().await;

        let projects = Project::find_local_projects_with_stats(&pool).await.unwrap();
        assert!(projects.is_empty());
    }

    #[tokio::test]
    async fn test_find_local_projects_with_stats() {
        let (pool, _temp_dir) = create_test_pool().await;

        let project_id = Uuid::new_v4();
        let create_data = CreateProject {
            name: "Stats Test".to_string(),
            git_repo_path: "/stats/with-counts/test".to_string(),
            use_existing_repo: true,
            clone_url: None,
            setup_script: None,
            dev_script: None,
            cleanup_script: None,
            copy_files: None,
        };

        Project::create(&pool, &create_data, project_id).await.unwrap();

        let projects = Project::find_local_projects_with_stats(&pool).await.unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].project.id, project_id);
        assert!(projects[0].last_attempt_at.is_none());
        assert_eq!(projects[0].task_counts.todo, 0);
        assert_eq!(projects[0].task_counts.in_progress, 0);
        assert_eq!(projects[0].task_counts.in_review, 0);
        assert_eq!(projects[0].task_counts.done, 0);
    }
}
