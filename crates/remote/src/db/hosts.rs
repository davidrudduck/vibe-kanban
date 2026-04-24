use relay_types::{HostRepo, RelayHost};
use sqlx::PgPool;
use uuid::Uuid;

pub struct HostRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> HostRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_accessible_hosts(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<RelayHost>, sqlx::Error> {
        sqlx::query_as!(
            RelayHost,
            r#"
            SELECT
                h.id,
                h.owner_user_id,
                h.machine_id AS "machine_id!",
                h.name,
                h.status,
                h.last_seen_at,
                h.agent_version,
                h.created_at,
                h.updated_at,
                CASE
                    WHEN h.owner_user_id = $1 THEN 'owner'
                    ELSE 'member'
                END AS "access_role!"
            FROM hosts h
            LEFT JOIN organization_member_metadata om
                ON om.organization_id = h.shared_with_organization_id
                AND om.user_id = $1
            WHERE h.owner_user_id = $1 OR om.user_id IS NOT NULL
            ORDER BY h.updated_at DESC
            "#,
            user_id
        )
        .fetch_all(self.pool)
        .await
    }

    pub async fn get_host_id_by_machine_id(
        &self,
        user_id: Uuid,
        machine_id: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        let row = sqlx::query!(
            "SELECT id FROM hosts WHERE owner_user_id = $1 AND machine_id = $2",
            user_id,
            machine_id
        )
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(|r| r.id))
    }

    pub async fn upsert_host_repos(
        &self,
        host_id: Uuid,
        repos: &[HostRepo],
    ) -> Result<(), sqlx::Error> {
        let paths: Vec<String> = repos.iter().map(|r| r.path.clone()).collect();
        // Remove repos no longer present on the host
        sqlx::query!(
            "DELETE FROM host_repos WHERE host_id = $1 AND NOT (path = ANY($2))",
            host_id,
            &paths as &[String]
        )
        .execute(self.pool)
        .await?;

        for repo in repos {
            sqlx::query!(
                r#"
                INSERT INTO host_repos (host_id, path, name, display_name)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (host_id, path) DO UPDATE
                SET name = EXCLUDED.name,
                    display_name = EXCLUDED.display_name,
                    updated_at = NOW()
                "#,
                host_id,
                repo.path,
                repo.name,
                repo.display_name
            )
            .execute(self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn list_host_repos(&self, host_id: Uuid) -> Result<Vec<HostRepo>, sqlx::Error> {
        let rows = sqlx::query!(
            "SELECT path, name, display_name FROM host_repos WHERE host_id = $1 ORDER BY name",
            host_id
        )
        .fetch_all(self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| HostRepo {
                path: r.path,
                name: r.name,
                display_name: r.display_name,
            })
            .collect())
    }
}
