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

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// Helper to get database URL from environment.
    fn database_url() -> Option<String> {
        std::env::var("SERVER_DATABASE_URL")
            .ok()
            .or_else(|| std::env::var("DATABASE_URL").ok())
    }

    /// Skip test if database is not available.
    macro_rules! skip_without_db {
        () => {
            if database_url().is_none() {
                eprintln!("Skipping test: DATABASE_URL or SERVER_DATABASE_URL not set");
                return;
            }
        };
    }

    /// Create a test database connection pool.
    async fn create_pool() -> PgPool {
        let url = database_url().expect("DATABASE_URL must be set");
        PgPool::connect(&url)
            .await
            .expect("Failed to connect to database")
    }

    /// Helper to insert test activity event
    async fn insert_test_activity(
        pool: &PgPool,
        swarm_project_id: Uuid,
        seq: i64,
        event_type: &str,
    ) -> Uuid {
        let event_id = Uuid::new_v4();
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO activity (event_id, swarm_project_id, seq, event_type, created_at, payload)
            VALUES ($1, $2, $3, $4, $5, '{}'::jsonb)
            "#,
        )
        .bind(event_id)
        .bind(swarm_project_id)
        .bind(seq)
        .bind(event_type)
        .bind(now)
        .execute(pool)
        .await
        .expect("Failed to insert test activity event");

        event_id
    }

    /// Cleanup helper - remove test activity events for a swarm project
    async fn cleanup_activity(pool: &PgPool, swarm_project_id: Uuid) {
        let _ = sqlx::query(
            r#"
            DELETE FROM activity
            WHERE swarm_project_id = $1
            "#,
        )
        .bind(swarm_project_id)
        .execute(pool)
        .await;
    }

    /// Test: fetch_since_by_swarm_project returns events in seq ASC order
    #[tokio::test]
    async fn test_fetch_since_by_swarm_project_returns_events() {
        skip_without_db!();

        let pool = create_pool().await;
        let swarm_project_id = Uuid::new_v4();

        // Insert test activity events
        insert_test_activity(&pool, swarm_project_id, 1, "task_created").await;
        insert_test_activity(&pool, swarm_project_id, 2, "task_updated").await;
        insert_test_activity(&pool, swarm_project_id, 3, "task_completed").await;

        // Create repository and fetch events
        let repo = ActivityRepository::new(&pool);
        let events = repo
            .fetch_since_by_swarm_project(swarm_project_id, None, 10)
            .await
            .expect("Failed to fetch events");

        // Assert: Returns events in seq ASC order
        assert_eq!(
            events.len(),
            3,
            "Should return all 3 events for the swarm project"
        );
        assert_eq!(events[0].seq, 1, "First event should have seq 1");
        assert_eq!(events[1].seq, 2, "Second event should have seq 2");
        assert_eq!(events[2].seq, 3, "Third event should have seq 3");
        assert_eq!(
            events[0].event_type, "task_created",
            "First event type should be task_created"
        );
        assert_eq!(
            events[1].event_type, "task_updated",
            "Second event type should be task_updated"
        );
        assert_eq!(
            events[2].event_type, "task_completed",
            "Third event type should be task_completed"
        );

        // Cleanup
        cleanup_activity(&pool, swarm_project_id).await;
    }

    /// Test: fetch_since_by_swarm_project pagination with after_seq parameter
    #[tokio::test]
    async fn test_fetch_since_by_swarm_project_pagination() {
        skip_without_db!();

        let pool = create_pool().await;
        let swarm_project_id = Uuid::new_v4();

        // Insert 5 events
        for i in 1..=5 {
            insert_test_activity(&pool, swarm_project_id, i, "test_event").await;
        }

        // Create repository
        let repo = ActivityRepository::new(&pool);

        // Fetch after seq 2 (should return events 3, 4, 5)
        let events = repo
            .fetch_since_by_swarm_project(swarm_project_id, Some(2), 10)
            .await
            .expect("Failed to fetch events");

        // Assert: Only returns events with seq > 2
        assert_eq!(
            events.len(),
            3,
            "Should return exactly 3 events after seq 2"
        );
        assert_eq!(events[0].seq, 3, "First event should have seq 3");
        assert_eq!(events[1].seq, 4, "Second event should have seq 4");
        assert_eq!(events[2].seq, 5, "Third event should have seq 5");

        // Cleanup
        cleanup_activity(&pool, swarm_project_id).await;
    }

    /// Test: fetch_since_by_swarm_project respects limit parameter
    #[tokio::test]
    async fn test_fetch_since_by_swarm_project_limit() {
        skip_without_db!();

        let pool = create_pool().await;
        let swarm_project_id = Uuid::new_v4();

        // Insert 10 events
        for i in 1..=10 {
            insert_test_activity(&pool, swarm_project_id, i, "test_event").await;
        }

        // Create repository
        let repo = ActivityRepository::new(&pool);

        // Fetch with limit 5
        let events = repo
            .fetch_since_by_swarm_project(swarm_project_id, None, 5)
            .await
            .expect("Failed to fetch events");

        // Assert: Only returns first 5 events
        assert_eq!(
            events.len(),
            5,
            "Should return exactly 5 events when limit is 5"
        );
        assert_eq!(events[0].seq, 1, "First event should have seq 1");
        assert_eq!(events[4].seq, 5, "Fifth event should have seq 5");

        // Cleanup
        cleanup_activity(&pool, swarm_project_id).await;
    }

    /// Test: fetch_since_by_swarm_project returns empty result when no events exist
    #[tokio::test]
    async fn test_fetch_since_by_swarm_project_empty_result() {
        skip_without_db!();

        let pool = create_pool().await;
        let swarm_project_id = Uuid::new_v4();

        // Create repository with no inserted events
        let repo = ActivityRepository::new(&pool);
        let events = repo
            .fetch_since_by_swarm_project(swarm_project_id, None, 10)
            .await
            .expect("Failed to fetch events");

        // Assert: Returns empty result
        assert_eq!(
            events.len(),
            0,
            "Should return no events when none exist for swarm project"
        );

        // Cleanup (no events to clean up)
    }
}
