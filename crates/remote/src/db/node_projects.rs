use chrono::Utc;
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use crate::nodes::NodeProject;

#[derive(Debug, Error)]
pub enum NodeProjectError {
    #[error("node project link not found")]
    NotFound,
    #[error("project already linked to a node")]
    ProjectAlreadyLinked,
    #[error("local project already linked on this node")]
    LocalProjectAlreadyLinked,
    #[error("project does not exist in hive - sync project before linking")]
    ProjectNotInHive,
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub struct NodeProjectRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> NodeProjectRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Link a project to a node
    pub async fn create(
        &self,
        node_id: Uuid,
        project_id: Uuid,
        local_project_id: Uuid,
        git_repo_path: &str,
        default_branch: &str,
    ) -> Result<NodeProject, NodeProjectError> {
        let link = sqlx::query_as::<_, NodeProject>(
            r#"
            INSERT INTO node_projects (node_id, project_id, local_project_id, git_repo_path, default_branch)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING
                id,
                node_id,
                project_id,
                local_project_id,
                git_repo_path,
                default_branch,
                sync_status,
                last_synced_at,
                created_at
            "#,
        )
        .bind(node_id)
        .bind(project_id)
        .bind(local_project_id)
        .bind(git_repo_path)
        .bind(default_branch)
        .fetch_one(self.pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e {
                // Note: node_projects_project_id_key constraint was removed in migration
                // 20251226000000_full_swarm_visibility.sql to allow multiple nodes per project
                if db_err.constraint() == Some("node_projects_node_id_local_project_id_key") {
                    return NodeProjectError::LocalProjectAlreadyLinked;
                }
                // Handle foreign key violation when project doesn't exist in hive
                if db_err.constraint() == Some("node_projects_project_id_fkey") {
                    return NodeProjectError::ProjectNotInHive;
                }
            }
            NodeProjectError::Database(e)
        })?;

        Ok(link)
    }

    /// Find a node project link by ID
    pub async fn find_by_id(&self, link_id: Uuid) -> Result<Option<NodeProject>, NodeProjectError> {
        let link = sqlx::query_as::<_, NodeProject>(
            r#"
            SELECT
                id,
                node_id,
                project_id,
                local_project_id,
                git_repo_path,
                default_branch,
                sync_status,
                last_synced_at,
                created_at
            FROM node_projects
            WHERE id = $1
            "#,
        )
        .bind(link_id)
        .fetch_optional(self.pool)
        .await?;

        Ok(link)
    }

    /// Find a node project link by project ID (returns first match only)
    pub async fn find_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Option<NodeProject>, NodeProjectError> {
        let link = sqlx::query_as::<_, NodeProject>(
            r#"
            SELECT
                id,
                node_id,
                project_id,
                local_project_id,
                git_repo_path,
                default_branch,
                sync_status,
                last_synced_at,
                created_at
            FROM node_projects
            WHERE project_id = $1
            "#,
        )
        .bind(project_id)
        .fetch_optional(self.pool)
        .await?;

        Ok(link)
    }

    /// Find ALL node project links by project ID (supports multi-node projects)
    ///
    /// Returns all nodes that have this project linked, enabling execution
    /// on any node that has a local copy of the project.
    pub async fn find_all_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<NodeProject>, NodeProjectError> {
        let links = sqlx::query_as::<_, NodeProject>(
            r#"
            SELECT
                id,
                node_id,
                project_id,
                local_project_id,
                git_repo_path,
                default_branch,
                sync_status,
                last_synced_at,
                created_at
            FROM node_projects
            WHERE project_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(project_id)
        .fetch_all(self.pool)
        .await?;

        Ok(links)
    }

    /// List all project links for a node
    pub async fn list_by_node(&self, node_id: Uuid) -> Result<Vec<NodeProject>, NodeProjectError> {
        let links = sqlx::query_as::<_, NodeProject>(
            r#"
            SELECT
                id,
                node_id,
                project_id,
                local_project_id,
                git_repo_path,
                default_branch,
                sync_status,
                last_synced_at,
                created_at
            FROM node_projects
            WHERE node_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(node_id)
        .fetch_all(self.pool)
        .await?;

        Ok(links)
    }

    /// List project links for a node that are linked to a swarm project.
    /// Only projects in `swarm_project_nodes` are returned - unlinked projects are excluded.
    pub async fn list_linked_by_node(
        &self,
        node_id: Uuid,
    ) -> Result<Vec<NodeProject>, NodeProjectError> {
        let links = sqlx::query_as::<_, NodeProject>(
            r#"
            SELECT
                np.id,
                np.node_id,
                np.project_id,
                np.local_project_id,
                np.git_repo_path,
                np.default_branch,
                np.sync_status,
                np.last_synced_at,
                np.created_at
            FROM node_projects np
            INNER JOIN swarm_project_nodes spn
                ON spn.node_id = np.node_id
                AND spn.local_project_id = np.local_project_id
            WHERE np.node_id = $1
            ORDER BY np.created_at DESC
            "#,
        )
        .bind(node_id)
        .fetch_all(self.pool)
        .await?;

        Ok(links)
    }

    /// Update sync status
    pub async fn update_sync_status(
        &self,
        link_id: Uuid,
        sync_status: &str,
    ) -> Result<(), NodeProjectError> {
        let result = sqlx::query(
            r#"
            UPDATE node_projects
            SET sync_status = $2,
                last_synced_at = $3
            WHERE id = $1
            "#,
        )
        .bind(link_id)
        .bind(sync_status)
        .bind(Utc::now())
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(NodeProjectError::NotFound);
        }

        Ok(())
    }

    /// Delete a project link
    pub async fn delete(&self, link_id: Uuid) -> Result<(), NodeProjectError> {
        let result = sqlx::query(
            r#"
            DELETE FROM node_projects
            WHERE id = $1
            "#,
        )
        .bind(link_id)
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(NodeProjectError::NotFound);
        }

        Ok(())
    }

    /// Delete a project link by project ID
    pub async fn delete_by_project(&self, project_id: Uuid) -> Result<(), NodeProjectError> {
        sqlx::query(
            r#"
            DELETE FROM node_projects
            WHERE project_id = $1
            "#,
        )
        .bind(project_id)
        .execute(self.pool)
        .await?;

        Ok(())
    }

    /// Find a node project link by node ID and project ID.
    pub async fn find_by_node_and_project(
        &self,
        node_id: Uuid,
        project_id: Uuid,
    ) -> Result<Option<NodeProject>, NodeProjectError> {
        let link = sqlx::query_as::<_, NodeProject>(
            r#"
            SELECT
                id,
                node_id,
                project_id,
                local_project_id,
                git_repo_path,
                default_branch,
                sync_status,
                last_synced_at,
                created_at
            FROM node_projects
            WHERE node_id = $1 AND project_id = $2
            "#,
        )
        .bind(node_id)
        .bind(project_id)
        .fetch_optional(self.pool)
        .await?;

        Ok(link)
    }

    /// Delete a project link by node ID and project ID.
    ///
    /// This ensures the node owns the project link before deleting it.
    pub async fn delete_by_node_and_project(
        &self,
        node_id: Uuid,
        project_id: Uuid,
    ) -> Result<(), NodeProjectError> {
        let result = sqlx::query(
            r#"
            DELETE FROM node_projects
            WHERE node_id = $1 AND project_id = $2
            "#,
        )
        .bind(node_id)
        .bind(project_id)
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(NodeProjectError::NotFound);
        }

        Ok(())
    }

    /// Bulk update node_id for all projects belonging to a source node.
    ///
    /// Used when merging nodes - moves all project links from source to target.
    /// Returns the number of projects that were updated.
    pub async fn bulk_update_node_id(
        &self,
        source_node_id: Uuid,
        target_node_id: Uuid,
    ) -> Result<u64, NodeProjectError> {
        let result = sqlx::query(
            r#"
            UPDATE node_projects
            SET node_id = $2
            WHERE node_id = $1
            "#,
        )
        .bind(source_node_id)
        .bind(target_node_id)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// List project links for an organization including the owning node's info.
    ///
    /// Each item represents a node-level link to a swarm project and includes the
    /// link id, project id, optional local project id, repository path, default
    /// branch, a display name for the project (uses the local project name when
    /// available, otherwise the repository name derived from the path), and the
    /// source node's id and name.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use uuid::Uuid;
    /// // `repo` is a NodeProjectRepository instance
    /// let org_id = Uuid::parse_str("00000000-0000-0000-0000-000000000000")?;
    /// let projects = repo.list_by_organization(org_id).await?;
    /// for p in projects {
    ///     println!("{} -> {}", p.project_name, p.source_node_name);
    /// }
    /// # Ok(()) }
    /// ```
    pub async fn list_by_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<OrgProjectInfo>, NodeProjectError> {
        let projects = sqlx::query_as::<_, OrgProjectInfo>(
            r#"
            SELECT
                np.id as link_id,
                np.project_id,
                np.local_project_id,
                np.git_repo_path,
                np.default_branch,
                COALESCE(nlp.name, SUBSTRING(np.git_repo_path FROM '[^/]+$')) as project_name,
                n.id as source_node_id,
                n.name as source_node_name
            FROM node_projects np
            JOIN nodes n ON np.node_id = n.id
            LEFT JOIN node_local_projects nlp ON np.node_id = nlp.node_id
                AND np.local_project_id = nlp.local_project_id
            WHERE n.organization_id = $1
            ORDER BY np.created_at DESC
            "#,
        )
        .bind(organization_id)
        .fetch_all(self.pool)
        .await?;

        Ok(projects)
    }

    /// Look up the organization that owns a project by querying node_projects and nodes.
    ///
    /// Returns the `organization_id` of the node linked to the given `project_id`, or `None` if no link exists.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use sqlx::PgPool;
    /// use uuid::Uuid;
    /// // assume `pool` is an initialized PgPool and `project_id` is a valid project UUID
    /// let pool: PgPool = unimplemented!();
    /// let project_id = Uuid::nil();
    /// let org = crate::db::node_projects::NodeProjectRepository::organization_id(&pool, project_id).await?;
    /// // `org` is `Some(uuid)` when a node owning the project exists, otherwise `None`
    /// # Ok(()) }
    /// ```
    pub async fn organization_id(
        pool: &PgPool,
        project_id: Uuid,
    ) -> Result<Option<Uuid>, NodeProjectError> {
        let result = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT n.organization_id
            FROM node_projects np
            JOIN nodes n ON np.node_id = n.id
            WHERE np.project_id = $1
            LIMIT 1
            "#,
        )
        .bind(project_id)
        .fetch_optional(pool)
        .await?;

        Ok(result)
    }
}

/// Project info with ownership details for organization-wide visibility.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OrgProjectInfo {
    pub link_id: Uuid,
    pub project_id: Uuid,
    pub local_project_id: Uuid,
    pub git_repo_path: String,
    pub default_branch: String,
    pub project_name: String,
    pub source_node_id: Uuid,
    pub source_node_name: String,
}