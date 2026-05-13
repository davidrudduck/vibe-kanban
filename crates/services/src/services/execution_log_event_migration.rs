use db::models::{
    execution_log_event::{CreateExecutionLogEvent, ExecutionLogEvent, ExecutionLogEventType},
    execution_process_logs::ExecutionProcessLogs,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use utils::log_msg::LogMsg;
use uuid::Uuid;

use crate::services::execution_process::persist_canonical_log_msg;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionLogEventMigrationSummary {
    pub processed_executions: usize,
    pub migrated_events: usize,
    pub skipped_invalid_lines: usize,
}

pub async fn migrate_legacy_execution_logs_to_events(
    pool: &SqlitePool,
) -> anyhow::Result<ExecutionLogEventMigrationSummary> {
    let execution_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT DISTINCT execution_id FROM execution_process_logs")
            .fetch_all(pool)
            .await?;
    let mut summary = ExecutionLogEventMigrationSummary::default();

    for execution_id in execution_ids {
        summary.processed_executions += 1;
        let records = ExecutionProcessLogs::find_by_execution_id(pool, execution_id).await?;
        let mut skipped_invalid_lines = 0_usize;

        for (sequence, line) in records
            .iter()
            .flat_map(|record| record.logs.lines())
            .filter(|line| !line.trim().is_empty())
            .enumerate()
        {
            match serde_json::from_str::<LogMsg>(line) {
                Ok(msg) => {
                    if persist_canonical_log_msg(
                        pool,
                        execution_id,
                        "legacy-jsonl",
                        sequence as u64,
                        &msg,
                    )
                    .await?
                    .is_some()
                    {
                        summary.migrated_events += 1;
                    }
                }
                Err(error) => {
                    skipped_invalid_lines += 1;
                    tracing::warn!(
                        %execution_id,
                        %error,
                        "Skipping invalid legacy execution log JSONL line"
                    );
                }
            }
        }

        if skipped_invalid_lines > 0 {
            summary.skipped_invalid_lines += skipped_invalid_lines;
            ExecutionLogEvent::create(
                pool,
                &CreateExecutionLogEvent {
                    execution_id,
                    source: "legacy-jsonl".to_string(),
                    source_event_id: Some("legacy-invalid-summary".to_string()),
                    event_type: ExecutionLogEventType::SystemStatus,
                    payload_json: serde_json::json!({
                        "kind": "migration_skipped_invalid_lines",
                        "skipped": skipped_invalid_lines,
                    }),
                },
            )
            .await?;
        }
    }

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use db::models::execution_log_event::{ExecutionLogEvent, ExecutionLogEventType};
    use serde_json::json;
    use sqlx::{
        SqlitePool,
        sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    };
    use utils::log_msg::LogMsg;
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
                id BLOB PRIMARY KEY,
                session_id BLOB NOT NULL
            );
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE execution_process_logs (
                execution_id BLOB NOT NULL,
                logs TEXT NOT NULL,
                byte_size INTEGER NOT NULL,
                inserted_at TEXT NOT NULL DEFAULT (datetime('now', 'subsec'))
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
        pool
    }

    async fn insert_legacy_logs(pool: &SqlitePool, execution_id: Uuid, lines: Vec<String>) {
        let session_id = Uuid::new_v4();
        sqlx::query("INSERT INTO execution_processes (id, session_id) VALUES (?, ?)")
            .bind(execution_id)
            .bind(session_id)
            .execute(pool)
            .await
            .unwrap();
        let logs = lines.join("\n");
        sqlx::query(
            "INSERT INTO execution_process_logs (execution_id, logs, byte_size) VALUES (?, ?, ?)",
        )
        .bind(execution_id)
        .bind(&logs)
        .bind(logs.len() as i64)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn migrates_valid_jsonl_messages_to_event_rows() {
        let pool = setup_pool().await;
        let execution_id = Uuid::new_v4();
        insert_legacy_logs(
            &pool,
            execution_id,
            vec![
                serde_json::to_string(&LogMsg::Stdout("one".into())).unwrap(),
                serde_json::to_string(&LogMsg::Finished).unwrap(),
            ],
        )
        .await;

        let summary = migrate_legacy_execution_logs_to_events(&pool)
            .await
            .unwrap();
        let events = ExecutionLogEvent::find_after_id(&pool, execution_id, None, 10)
            .await
            .unwrap();

        assert_eq!(summary.migrated_events, 2);
        assert_eq!(
            events
                .iter()
                .map(|event| event.event_type)
                .collect::<Vec<_>>(),
            vec![
                ExecutionLogEventType::RawStdout,
                ExecutionLogEventType::ExecutionFinished,
            ]
        );
    }

    #[tokio::test]
    async fn records_malformed_jsonl_as_skipped_count() {
        let pool = setup_pool().await;
        let execution_id = Uuid::new_v4();
        insert_legacy_logs(
            &pool,
            execution_id,
            vec![
                serde_json::to_string(&LogMsg::Stdout("valid".into())).unwrap(),
                "{bad json".to_string(),
            ],
        )
        .await;

        let summary = migrate_legacy_execution_logs_to_events(&pool)
            .await
            .unwrap();
        let events = ExecutionLogEvent::find_after_id(&pool, execution_id, None, 10)
            .await
            .unwrap();

        assert_eq!(summary.skipped_invalid_lines, 1);
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].event_type, ExecutionLogEventType::SystemStatus);
        assert_eq!(
            events[1].payload_json,
            json!({"kind":"migration_skipped_invalid_lines","skipped":1})
        );
    }

    #[tokio::test]
    async fn migration_is_idempotent() {
        let pool = setup_pool().await;
        let execution_id = Uuid::new_v4();
        insert_legacy_logs(
            &pool,
            execution_id,
            vec![serde_json::to_string(&LogMsg::Stdout("one".into())).unwrap()],
        )
        .await;

        migrate_legacy_execution_logs_to_events(&pool)
            .await
            .unwrap();
        migrate_legacy_execution_logs_to_events(&pool)
            .await
            .unwrap();
        let events = ExecutionLogEvent::find_after_id(&pool, execution_id, None, 10)
            .await
            .unwrap();

        assert_eq!(events.len(), 1);
    }
}
