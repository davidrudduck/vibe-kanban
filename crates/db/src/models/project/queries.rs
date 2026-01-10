//! CRUD query operations for projects.

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use super::{CreateProject, Project};

impl Project {
    pub async fn count(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar!(r#"SELECT COUNT(*) as "count!: i64" FROM projects"#)
            .fetch_one(pool)
            .await
    }

    pub async fn find_all(pool: &SqlitePool) -> Result<Vec<Self>, sqlx::Error> {
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
               ORDER BY created_at DESC"#
        )
        .fetch_all(pool)
        .await
    }

    /// Find the most actively used projects based on recent task activity
    pub async fn find_most_active(pool: &SqlitePool, limit: i32) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Project,
            r#"
            SELECT p.id as "id!: Uuid", p.name, p.git_repo_path, p.setup_script, p.dev_script, p.cleanup_script, p.copy_files,
                   p.parallel_setup_script as "parallel_setup_script!: bool",
                   p.remote_project_id as "remote_project_id: Uuid",
                   p.created_at as "created_at!: DateTime<Utc>", p.updated_at as "updated_at!: DateTime<Utc>",
                   p.is_remote as "is_remote!: bool",
                   p.source_node_id as "source_node_id: Uuid",
                   p.source_node_name, p.source_node_public_url, p.source_node_status,
                   p.remote_last_synced_at as "remote_last_synced_at: DateTime<Utc>",
                   p.github_enabled as "github_enabled!: bool",
                   p.github_owner,
                   p.github_repo,
                   p.github_open_issues as "github_open_issues!: i32",
                   p.github_open_prs as "github_open_prs!: i32",
                   p.github_last_synced_at as "github_last_synced_at: DateTime<Utc>"
            FROM projects p
            WHERE p.id IN (
                SELECT DISTINCT t.project_id
                FROM tasks t
                INNER JOIN task_attempts ta ON ta.task_id = t.id
                ORDER BY ta.updated_at DESC
            )
            LIMIT $1
            "#,
            limit
        )
        .fetch_all(pool)
        .await
    }

    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
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
               WHERE id = $1"#,
            id
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_git_repo_path(
        pool: &SqlitePool,
        git_repo_path: &str,
    ) -> Result<Option<Self>, sqlx::Error> {
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
               WHERE git_repo_path = $1"#,
            git_repo_path
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_git_repo_path_excluding_id(
        pool: &SqlitePool,
        git_repo_path: &str,
        exclude_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
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
               WHERE git_repo_path = $1 AND id != $2"#,
            git_repo_path,
            exclude_id
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn create(
        pool: &SqlitePool,
        data: &CreateProject,
        project_id: Uuid,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as!(
            Project,
            r#"INSERT INTO projects (
                    id,
                    name,
                    git_repo_path,
                    setup_script,
                    dev_script,
                    cleanup_script,
                    copy_files
                ) VALUES (
                    $1, $2, $3, $4, $5, $6, $7
                )
                RETURNING id as "id!: Uuid",
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
                          github_last_synced_at as "github_last_synced_at: DateTime<Utc>""#,
            project_id,
            data.name,
            data.git_repo_path,
            data.setup_script,
            data.dev_script,
            data.cleanup_script,
            data.copy_files,
        )
        .fetch_one(pool)
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update(
        pool: &SqlitePool,
        id: Uuid,
        name: String,
        git_repo_path: String,
        setup_script: Option<String>,
        dev_script: Option<String>,
        cleanup_script: Option<String>,
        copy_files: Option<String>,
        parallel_setup_script: bool,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as!(
            Project,
            r#"UPDATE projects
               SET name = $2,
                   git_repo_path = $3,
                   setup_script = $4,
                   dev_script = $5,
                   cleanup_script = $6,
                   copy_files = $7,
                   parallel_setup_script = $8
               WHERE id = $1
               RETURNING id as "id!: Uuid",
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
                         github_last_synced_at as "github_last_synced_at: DateTime<Utc>""#,
            id,
            name,
            git_repo_path,
            setup_script,
            dev_script,
            cleanup_script,
            copy_files,
            parallel_setup_script,
        )
        .fetch_one(pool)
        .await
    }

    pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!("DELETE FROM projects WHERE id = $1", id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn exists(pool: &SqlitePool, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
                SELECT COUNT(*) as "count!: i64"
                FROM projects
                WHERE id = $1
            "#,
            id
        )
        .fetch_one(pool)
        .await?;

        Ok(result.count > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_test_pool;

    #[tokio::test]
    async fn test_project_count() {
        let (pool, _temp_dir) = create_test_pool().await;

        let count = Project::count(&pool).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_project_find_all_empty() {
        let (pool, _temp_dir) = create_test_pool().await;

        let projects = Project::find_all(&pool).await.unwrap();
        assert!(projects.is_empty());
    }

    #[tokio::test]
    async fn test_project_create_and_find_by_id() {
        let (pool, _temp_dir) = create_test_pool().await;

        let project_id = Uuid::new_v4();
        let create_data = CreateProject {
            name: "Test Project".to_string(),
            git_repo_path: "/tmp/test-repo".to_string(),
            use_existing_repo: true,
            clone_url: None,
            setup_script: Some("npm install".to_string()),
            dev_script: Some("npm run dev".to_string()),
            cleanup_script: None,
            copy_files: None,
        };

        let created = Project::create(&pool, &create_data, project_id)
            .await
            .unwrap();
        assert_eq!(created.id, project_id);
        assert_eq!(created.name, "Test Project");
        assert_eq!(created.git_repo_path.to_string_lossy(), "/tmp/test-repo");

        let found = Project::find_by_id(&pool, project_id).await.unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.id, project_id);
        assert_eq!(found.name, "Test Project");
    }

    #[tokio::test]
    async fn test_project_find_by_git_repo_path() {
        let (pool, _temp_dir) = create_test_pool().await;

        let project_id = Uuid::new_v4();
        let create_data = CreateProject {
            name: "Path Test".to_string(),
            git_repo_path: "/unique/path/test".to_string(),
            use_existing_repo: true,
            clone_url: None,
            setup_script: None,
            dev_script: None,
            cleanup_script: None,
            copy_files: None,
        };

        Project::create(&pool, &create_data, project_id)
            .await
            .unwrap();

        let found = Project::find_by_git_repo_path(&pool, "/unique/path/test")
            .await
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, project_id);

        let not_found = Project::find_by_git_repo_path(&pool, "/nonexistent")
            .await
            .unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_project_find_by_git_repo_path_excluding_id() {
        let (pool, _temp_dir) = create_test_pool().await;

        let project_id = Uuid::new_v4();
        let create_data = CreateProject {
            name: "Exclude Test".to_string(),
            git_repo_path: "/exclude/path/test".to_string(),
            use_existing_repo: true,
            clone_url: None,
            setup_script: None,
            dev_script: None,
            cleanup_script: None,
            copy_files: None,
        };

        Project::create(&pool, &create_data, project_id)
            .await
            .unwrap();

        // Should not find when excluding the same ID
        let not_found =
            Project::find_by_git_repo_path_excluding_id(&pool, "/exclude/path/test", project_id)
                .await
                .unwrap();
        assert!(not_found.is_none());

        // Should find when excluding a different ID
        let found = Project::find_by_git_repo_path_excluding_id(
            &pool,
            "/exclude/path/test",
            Uuid::new_v4(),
        )
        .await
        .unwrap();
        assert!(found.is_some());
    }

    #[tokio::test]
    async fn test_project_update() {
        let (pool, _temp_dir) = create_test_pool().await;

        let project_id = Uuid::new_v4();
        let create_data = CreateProject {
            name: "Original Name".to_string(),
            git_repo_path: "/update/test".to_string(),
            use_existing_repo: true,
            clone_url: None,
            setup_script: None,
            dev_script: None,
            cleanup_script: None,
            copy_files: None,
        };

        Project::create(&pool, &create_data, project_id)
            .await
            .unwrap();

        let updated = Project::update(
            &pool,
            project_id,
            "Updated Name".to_string(),
            "/update/test".to_string(),
            Some("new setup".to_string()),
            Some("new dev".to_string()),
            None,
            None,
            true,
        )
        .await
        .unwrap();

        assert_eq!(updated.name, "Updated Name");
        assert_eq!(updated.setup_script, Some("new setup".to_string()));
        assert_eq!(updated.dev_script, Some("new dev".to_string()));
        assert!(updated.parallel_setup_script);
    }

    #[tokio::test]
    async fn test_project_delete() {
        let (pool, _temp_dir) = create_test_pool().await;

        let project_id = Uuid::new_v4();
        let create_data = CreateProject {
            name: "Delete Test".to_string(),
            git_repo_path: "/delete/test".to_string(),
            use_existing_repo: true,
            clone_url: None,
            setup_script: None,
            dev_script: None,
            cleanup_script: None,
            copy_files: None,
        };

        Project::create(&pool, &create_data, project_id)
            .await
            .unwrap();

        let deleted = Project::delete(&pool, project_id).await.unwrap();
        assert_eq!(deleted, 1);

        let not_found = Project::find_by_id(&pool, project_id).await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_project_exists() {
        let (pool, _temp_dir) = create_test_pool().await;

        let project_id = Uuid::new_v4();
        let create_data = CreateProject {
            name: "Exists Test".to_string(),
            git_repo_path: "/exists/test".to_string(),
            use_existing_repo: true,
            clone_url: None,
            setup_script: None,
            dev_script: None,
            cleanup_script: None,
            copy_files: None,
        };

        assert!(!Project::exists(&pool, project_id).await.unwrap());

        Project::create(&pool, &create_data, project_id)
            .await
            .unwrap();

        assert!(Project::exists(&pool, project_id).await.unwrap());
    }
}
