use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Row, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ExecutionLogEventType {
    ExecutionStarted,
    ExecutionFinished,
    UserMessage,
    AssistantMessageDelta,
    AssistantMessageFinal,
    ToolStarted,
    ToolDelta,
    ToolFinished,
    SystemStatus,
    RawStdout,
    RawStderr,
    JsonPatch,
    ResetIgnored,
    RefreshRequired,
}

impl ExecutionLogEventType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExecutionStarted => "execution_started",
            Self::ExecutionFinished => "execution_finished",
            Self::UserMessage => "user_message",
            Self::AssistantMessageDelta => "assistant_message_delta",
            Self::AssistantMessageFinal => "assistant_message_final",
            Self::ToolStarted => "tool_started",
            Self::ToolDelta => "tool_delta",
            Self::ToolFinished => "tool_finished",
            Self::SystemStatus => "system_status",
            Self::RawStdout => "raw_stdout",
            Self::RawStderr => "raw_stderr",
            Self::JsonPatch => "json_patch",
            Self::ResetIgnored => "reset_ignored",
            Self::RefreshRequired => "refresh_required",
        }
    }

    fn from_db(value: &str) -> Result<Self, sqlx::Error> {
        match value {
            "execution_started" => Ok(Self::ExecutionStarted),
            "execution_finished" => Ok(Self::ExecutionFinished),
            "user_message" => Ok(Self::UserMessage),
            "assistant_message_delta" => Ok(Self::AssistantMessageDelta),
            "assistant_message_final" => Ok(Self::AssistantMessageFinal),
            "tool_started" => Ok(Self::ToolStarted),
            "tool_delta" => Ok(Self::ToolDelta),
            "tool_finished" => Ok(Self::ToolFinished),
            "system_status" => Ok(Self::SystemStatus),
            "raw_stdout" => Ok(Self::RawStdout),
            "raw_stderr" => Ok(Self::RawStderr),
            "json_patch" => Ok(Self::JsonPatch),
            "reset_ignored" => Ok(Self::ResetIgnored),
            "refresh_required" => Ok(Self::RefreshRequired),
            other => Err(sqlx::Error::Decode(
                format!("unknown execution log event type: {other}").into(),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Direction {
    Forward,
    Backward,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExecutionLogEvent {
    pub id: i64,
    #[ts(type = "string")]
    pub execution_id: Uuid,
    pub source: String,
    pub source_event_id: Option<String>,
    pub event_type: ExecutionLogEventType,
    pub payload_json: Value,
    #[ts(type = "string")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CreateExecutionLogEvent {
    #[ts(type = "string")]
    pub execution_id: Uuid,
    pub source: String,
    pub source_event_id: Option<String>,
    pub event_type: ExecutionLogEventType,
    pub payload_json: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PaginatedExecutionLogEvents {
    pub entries: Vec<ExecutionLogEvent>,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}

impl ExecutionLogEvent {
    fn from_row(row: sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        let event_type: String = row.try_get("event_type")?;
        let payload: String = row.try_get("payload_json")?;
        let payload_json = serde_json::from_str(&payload).map_err(|error| {
            sqlx::Error::Decode(format!("invalid execution log payload JSON: {error}").into())
        })?;

        Ok(Self {
            id: row.try_get("id")?,
            execution_id: row.try_get("execution_id")?,
            source: row.try_get("source")?,
            source_event_id: row.try_get("source_event_id")?,
            event_type: ExecutionLogEventType::from_db(&event_type)?,
            payload_json,
            created_at: row.try_get("created_at")?,
        })
    }

    pub async fn create(
        pool: &SqlitePool,
        data: &CreateExecutionLogEvent,
    ) -> Result<Self, sqlx::Error> {
        let payload = serde_json::to_string(&data.payload_json)
            .map_err(|error| sqlx::Error::Encode(error.into()))?;
        let result = sqlx::query(
            r#"
            INSERT INTO execution_log_events (
                execution_id,
                source,
                source_event_id,
                event_type,
                payload_json
            )
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(execution_id, source, source_event_id) DO NOTHING
            "#,
        )
        .bind(data.execution_id)
        .bind(&data.source)
        .bind(&data.source_event_id)
        .bind(data.event_type.as_str())
        .bind(payload)
        .execute(pool)
        .await?;

        if result.rows_affected() == 0 {
            if let Some(source_event_id) = &data.source_event_id {
                return Self::find_by_source_event_id(
                    pool,
                    data.execution_id,
                    &data.source,
                    source_event_id,
                )
                .await?
                .ok_or(sqlx::Error::RowNotFound);
            }
        }

        Self::find_by_id(pool, result.last_insert_rowid()).await
    }

    pub async fn create_many(
        pool: &SqlitePool,
        events: &[CreateExecutionLogEvent],
    ) -> Result<Vec<Self>, sqlx::Error> {
        let mut created = Vec::with_capacity(events.len());
        for event in events {
            created.push(Self::create(pool, event).await?);
        }
        Ok(created)
    }

    pub async fn find_by_id(pool: &SqlitePool, id: i64) -> Result<Self, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT id, execution_id, source, source_event_id, event_type, payload_json, created_at
            FROM execution_log_events
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_one(pool)
        .await?;
        Self::from_row(row)
    }

    pub async fn find_by_source_event_id(
        pool: &SqlitePool,
        execution_id: Uuid,
        source: &str,
        source_event_id: &str,
    ) -> Result<Option<Self>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT id, execution_id, source, source_event_id, event_type, payload_json, created_at
            FROM execution_log_events
            WHERE execution_id = ? AND source = ? AND source_event_id = ?
            "#,
        )
        .bind(execution_id)
        .bind(source)
        .bind(source_event_id)
        .fetch_optional(pool)
        .await?;

        row.map(Self::from_row).transpose()
    }

    pub async fn find_after_id(
        pool: &SqlitePool,
        execution_id: Uuid,
        after_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT id, execution_id, source, source_event_id, event_type, payload_json, created_at
            FROM execution_log_events
            WHERE execution_id = ? AND (? IS NULL OR id > ?)
            ORDER BY id ASC
            LIMIT ?
            "#,
        )
        .bind(execution_id)
        .bind(after_id)
        .bind(after_id)
        .bind(limit.max(0))
        .fetch_all(pool)
        .await?;

        rows.into_iter().map(Self::from_row).collect()
    }

    pub async fn find_page(
        pool: &SqlitePool,
        execution_id: Uuid,
        cursor: Option<i64>,
        limit: i64,
        direction: Direction,
    ) -> Result<PaginatedExecutionLogEvents, sqlx::Error> {
        let limit = limit.clamp(1, 500);
        let fetch_limit = limit + 1;
        let rows = match direction {
            Direction::Forward => {
                sqlx::query(
                    r#"
                    SELECT id, execution_id, source, source_event_id, event_type, payload_json, created_at
                    FROM execution_log_events
                    WHERE execution_id = ? AND (? IS NULL OR id > ?)
                    ORDER BY id ASC
                    LIMIT ?
                    "#,
                )
                .bind(execution_id)
                .bind(cursor)
                .bind(cursor)
                .bind(fetch_limit)
                .fetch_all(pool)
                .await?
            }
            Direction::Backward => {
                sqlx::query(
                    r#"
                    SELECT id, execution_id, source, source_event_id, event_type, payload_json, created_at
                    FROM execution_log_events
                    WHERE execution_id = ? AND (? IS NULL OR id < ?)
                    ORDER BY id DESC
                    LIMIT ?
                    "#,
                )
                .bind(execution_id)
                .bind(cursor)
                .bind(cursor)
                .bind(fetch_limit)
                .fetch_all(pool)
                .await?
            }
        };

        let has_more = rows.len() > limit as usize;
        let mut entries = rows
            .into_iter()
            .take(limit as usize)
            .map(Self::from_row)
            .collect::<Result<Vec<_>, _>>()?;
        if direction == Direction::Backward {
            entries.reverse();
        }
        let next_cursor = match direction {
            Direction::Forward => entries.last().map(|entry| entry.id),
            Direction::Backward => entries.first().map(|entry| entry.id),
        };

        Ok(PaginatedExecutionLogEvents {
            entries,
            next_cursor,
            has_more,
        })
    }

    pub async fn find_last_id(
        pool: &SqlitePool,
        execution_id: Uuid,
    ) -> Result<Option<i64>, sqlx::Error> {
        sqlx::query_scalar(
            r#"
            SELECT MAX(id)
            FROM execution_log_events
            WHERE execution_id = ?
            "#,
        )
        .bind(execution_id)
        .fetch_one(pool)
        .await
    }

    pub async fn delete_by_execution_id(
        pool: &SqlitePool,
        execution_id: Uuid,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            DELETE FROM execution_log_events
            WHERE execution_id = ?
            "#,
        )
        .bind(execution_id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use sqlx::{
        SqlitePool,
        sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    };
    use uuid::Uuid;

    use super::*;

    async fn setup_pool() -> SqlitePool {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE execution_processes (
                id BLOB PRIMARY KEY
            );
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE execution_log_events (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                execution_id    BLOB NOT NULL,
                source          TEXT NOT NULL,
                source_event_id TEXT,
                event_type      TEXT NOT NULL CHECK (
                    event_type IN (
                        'execution_started',
                        'execution_finished',
                        'user_message',
                        'assistant_message_delta',
                        'assistant_message_final',
                        'tool_started',
                        'tool_delta',
                        'tool_finished',
                        'system_status',
                        'raw_stdout',
                        'raw_stderr',
                        'json_patch',
                        'reset_ignored',
                        'refresh_required'
                    )
                ),
                payload_json    TEXT NOT NULL,
                created_at      TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
                FOREIGN KEY (execution_id) REFERENCES execution_processes(id) ON DELETE CASCADE,
                UNIQUE(execution_id, source, source_event_id)
            );
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE INDEX idx_execution_log_events_execution_id_id ON execution_log_events(execution_id, id)",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    async fn create_execution(pool: &SqlitePool, execution_id: Uuid) {
        sqlx::query("INSERT INTO execution_processes (id) VALUES (?)")
            .bind(execution_id)
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn create_and_page_events_by_cursor_order() {
        let pool = setup_pool().await;
        let execution_id = Uuid::new_v4();
        create_execution(&pool, execution_id).await;

        let first = ExecutionLogEvent::create(
            &pool,
            &CreateExecutionLogEvent {
                execution_id,
                source: "test".to_string(),
                source_event_id: Some("one".to_string()),
                event_type: ExecutionLogEventType::RawStdout,
                payload_json: serde_json::json!({"text":"one"}),
            },
        )
        .await
        .unwrap();
        let second = ExecutionLogEvent::create(
            &pool,
            &CreateExecutionLogEvent {
                execution_id,
                source: "test".to_string(),
                source_event_id: Some("two".to_string()),
                event_type: ExecutionLogEventType::ExecutionFinished,
                payload_json: serde_json::json!({"exitCode":0}),
            },
        )
        .await
        .unwrap();

        let page = ExecutionLogEvent::find_after_id(&pool, execution_id, Some(first.id), 10)
            .await
            .unwrap();

        assert_eq!(page, vec![second]);
    }

    #[tokio::test]
    async fn source_event_id_uniqueness_makes_inserts_idempotent() {
        let pool = setup_pool().await;
        let execution_id = Uuid::new_v4();
        create_execution(&pool, execution_id).await;
        let event = CreateExecutionLogEvent {
            execution_id,
            source: "claude".to_string(),
            source_event_id: Some("event-1".to_string()),
            event_type: ExecutionLogEventType::AssistantMessageDelta,
            payload_json: serde_json::json!({"text":"hello"}),
        };

        let created = ExecutionLogEvent::create(&pool, &event).await.unwrap();
        let duplicate = ExecutionLogEvent::create(&pool, &event).await.unwrap();
        let all = ExecutionLogEvent::find_page(&pool, execution_id, None, 10, Direction::Forward)
            .await
            .unwrap();

        assert_eq!(created.id, duplicate.id);
        assert_eq!(all.entries, vec![created]);
        assert!(!all.has_more);
    }

    #[tokio::test]
    async fn backward_and_forward_pages_report_has_more() {
        let pool = setup_pool().await;
        let execution_id = Uuid::new_v4();
        create_execution(&pool, execution_id).await;
        let mut created = Vec::new();
        for index in 0..3 {
            created.push(
                ExecutionLogEvent::create(
                    &pool,
                    &CreateExecutionLogEvent {
                        execution_id,
                        source: "test".to_string(),
                        source_event_id: Some(format!("event-{index}")),
                        event_type: ExecutionLogEventType::SystemStatus,
                        payload_json: serde_json::json!({"index":index}),
                    },
                )
                .await
                .unwrap(),
            );
        }

        let first_page =
            ExecutionLogEvent::find_page(&pool, execution_id, None, 2, Direction::Forward)
                .await
                .unwrap();
        let last_page =
            ExecutionLogEvent::find_page(&pool, execution_id, None, 2, Direction::Backward)
                .await
                .unwrap();

        assert_eq!(first_page.entries, created[..2].to_vec());
        assert_eq!(first_page.next_cursor, Some(created[1].id));
        assert!(first_page.has_more);
        assert_eq!(last_page.entries, created[1..].to_vec());
        assert_eq!(last_page.next_cursor, Some(created[1].id));
        assert!(last_page.has_more);
    }

    #[tokio::test]
    async fn delete_by_execution_id_removes_only_that_execution() {
        let pool = setup_pool().await;
        let first_execution = Uuid::new_v4();
        let second_execution = Uuid::new_v4();
        create_execution(&pool, first_execution).await;
        create_execution(&pool, second_execution).await;

        ExecutionLogEvent::create(
            &pool,
            &CreateExecutionLogEvent {
                execution_id: first_execution,
                source: "test".to_string(),
                source_event_id: Some("one".to_string()),
                event_type: ExecutionLogEventType::RawStdout,
                payload_json: serde_json::json!({"text":"one"}),
            },
        )
        .await
        .unwrap();
        let survivor = ExecutionLogEvent::create(
            &pool,
            &CreateExecutionLogEvent {
                execution_id: second_execution,
                source: "test".to_string(),
                source_event_id: Some("two".to_string()),
                event_type: ExecutionLogEventType::RawStdout,
                payload_json: serde_json::json!({"text":"two"}),
            },
        )
        .await
        .unwrap();

        let deleted = ExecutionLogEvent::delete_by_execution_id(&pool, first_execution)
            .await
            .unwrap();
        let remaining = ExecutionLogEvent::find_after_id(&pool, second_execution, None, 10)
            .await
            .unwrap();

        assert_eq!(deleted, 1);
        assert_eq!(remaining, vec![survivor]);
    }
}
