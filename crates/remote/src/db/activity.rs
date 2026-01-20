use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::activity::ActivityEvent;

pub struct ActivityRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> ActivityRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn fetch_since(
        &self,
        project_id: Uuid,
        after_seq: Option<i64>,
        limit: i64,
    ) -> Result<Vec<ActivityEvent>, sqlx::Error> {
        let rows = sqlx::query_as::<_, ActivityRow>(
            r#"
            SELECT seq,
                   event_id,
                   project_id,
                   event_type,
                   created_at,
                   payload
            FROM activity
            WHERE project_id = $1
              AND ($2::bigint IS NULL OR seq > $2)
            ORDER BY seq ASC
            LIMIT $3
            "#,
        )
        .bind(project_id)
        .bind(after_seq)
        .bind(limit)
        .fetch_all(self.pool)
        .await?;

        Ok(rows.into_iter().map(ActivityRow::into_event).collect())
    }

    pub async fn fetch_since_by_swarm_project(
        &self,
        swarm_project_id: Uuid,
        after_seq: Option<i64>,
        limit: i64,
    ) -> Result<Vec<ActivityEvent>, sqlx::Error> {
        let rows = sqlx::query_as::<_, ActivityRow>(
            r#"
            SELECT seq,
                   event_id,
                   project_id,
                   event_type,
                   created_at,
                   payload
            FROM activity
            WHERE swarm_project_id = $1
              AND ($2::bigint IS NULL OR seq > $2)
            ORDER BY seq ASC
            LIMIT $3
            "#,
        )
        .bind(swarm_project_id)
        .bind(after_seq)
        .bind(limit)
        .fetch_all(self.pool)
        .await?;

        Ok(rows.into_iter().map(ActivityRow::into_event).collect())
    }

    pub async fn fetch_by_seq(
        &self,
        project_id: Uuid,
        seq: i64,
    ) -> Result<Option<ActivityEvent>, sqlx::Error> {
        let row = sqlx::query_as::<_, ActivityRow>(
            r#"
            SELECT seq,
                   event_id,
                   project_id,
                   event_type,
                   created_at,
                   payload
            FROM activity
            WHERE project_id = $1
              AND seq = $2
            LIMIT 1
            "#,
        )
        .bind(project_id)
        .bind(seq)
        .fetch_optional(self.pool)
        .await?;

        Ok(row.map(ActivityRow::into_event))
    }
}

#[derive(sqlx::FromRow)]
struct ActivityRow {
    seq: i64,
    event_id: Uuid,
    project_id: Uuid,
    event_type: String,
    created_at: DateTime<Utc>,
    payload: serde_json::Value,
}

impl ActivityRow {
    fn into_event(self) -> ActivityEvent {
        ActivityEvent::new(
            self.seq,
            self.event_id,
            self.project_id,
            self.event_type,
            self.created_at,
            Some(self.payload),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_activity_repository_fetch_since_by_swarm_project_signature() {
        // This test verifies that the fetch_since_by_swarm_project method
        // has the correct signature and is accessible
        // The actual async test would require a database pool
        // For now, we verify the method exists on the struct through compilation
        let _fn_ref = ActivityRepository::fetch_since_by_swarm_project;
        let _ = _fn_ref; // Use reference to avoid unused warning
    }

    #[test]
    fn test_activity_row_conversion() {
        // Verify ActivityRow can be converted to ActivityEvent
        let row = ActivityRow {
            seq: 1,
            event_id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            event_type: "test_event".to_string(),
            created_at: chrono::Utc::now(),
            payload: serde_json::json!({"key": "value"}),
        };

        let event = row.into_event();
        assert_eq!(event.seq, 1);
        assert_eq!(event.event_type, "test_event");
    }
}
