use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use utils::log_msg::LogMsg;
use utils::unified_log::{Direction, LogEntry, PaginatedLogs};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct ExecutionProcessLogs {
    pub execution_id: Uuid,
    pub logs: String, // JSONL format
    pub byte_size: i64,
    pub inserted_at: DateTime<Utc>,
}

impl ExecutionProcessLogs {
    /// Find logs by execution process ID
    pub async fn find_by_execution_id(
        pool: &SqlitePool,
        execution_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            ExecutionProcessLogs,
            r#"SELECT 
                execution_id as "execution_id!: Uuid",
                logs,
                byte_size,
                inserted_at as "inserted_at!: DateTime<Utc>"
               FROM execution_process_logs 
               WHERE execution_id = $1
               ORDER BY inserted_at ASC"#,
            execution_id
        )
        .fetch_all(pool)
        .await
    }

    /// Parse JSONL logs back into Vec<LogMsg>
    pub fn parse_logs(records: &[Self]) -> Result<Vec<LogMsg>, serde_json::Error> {
        let mut messages = Vec::new();
        for line in records.iter().flat_map(|record| record.logs.lines()) {
            if !line.trim().is_empty() {
                let msg: LogMsg = serde_json::from_str(line)?;
                messages.push(msg);
            }
        }
        Ok(messages)
    }

    /// Convert Vec<LogMsg> to JSONL format
    pub fn serialize_logs(messages: &[LogMsg]) -> Result<String, serde_json::Error> {
        let mut jsonl = String::new();
        for msg in messages {
            let line = serde_json::to_string(msg)?;
            jsonl.push_str(&line);
            jsonl.push('\n');
        }
        Ok(jsonl)
    }

    /// Append a JSONL line to the logs for an execution process
    pub async fn append_log_line(
        pool: &SqlitePool,
        execution_id: Uuid,
        jsonl_line: &str,
    ) -> Result<(), sqlx::Error> {
        let byte_size = jsonl_line.len() as i64;
        sqlx::query!(
            r#"INSERT INTO execution_process_logs (execution_id, logs, byte_size, inserted_at)
               VALUES ($1, $2, $3, datetime('now', 'subsec'))"#,
            execution_id,
            jsonl_line,
            byte_size
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Find paginated log entries for an execution process.
    ///
    /// This method fetches log records from the database and converts the JSONL
    /// content into individual `LogEntry` items with sequential IDs.
    ///
    /// # Arguments
    /// * `pool` - Database connection pool
    /// * `execution_id` - The execution process ID to fetch logs for
    /// * `cursor` - Optional cursor (line ID) to start from
    /// * `limit` - Maximum number of entries to return
    /// * `direction` - Forward (oldest first) or Backward (newest first)
    ///
    /// # Returns
    /// A `PaginatedLogs` struct containing the entries and pagination info.
    pub async fn find_paginated(
        pool: &SqlitePool,
        execution_id: Uuid,
        cursor: Option<i64>,
        limit: i64,
        direction: Direction,
    ) -> Result<PaginatedLogs, sqlx::Error> {
        // Fetch all log records for this execution
        let records = Self::find_by_execution_id(pool, execution_id).await?;

        // Parse all JSONL lines into LogEntry items with sequential IDs
        let all_entries = Self::parse_to_entries(&records, execution_id);
        let total_count = all_entries.len() as i64;

        // Apply pagination based on cursor and direction
        let paginated = Self::apply_pagination(all_entries, cursor, limit, direction);

        Ok(paginated.with_total_count(total_count))
    }

    /// Parse log records into LogEntry items with sequential IDs.
    ///
    /// Each line in the JSONL is assigned a sequential ID starting from 1.
    /// The timestamp for each entry comes from the record's inserted_at field.
    pub fn parse_to_entries(records: &[Self], execution_id: Uuid) -> Vec<LogEntry> {
        let mut entries = Vec::new();
        let mut id: i64 = 1;

        for record in records {
            for line in record.logs.lines() {
                if line.trim().is_empty() {
                    continue;
                }

                match LogEntry::from_local_jsonl(line, id, execution_id, record.inserted_at) {
                    Ok(entry) => {
                        entries.push(entry);
                        id += 1;
                    }
                    Err(e) => {
                        // Log parsing errors but continue processing
                        tracing::warn!(
                            execution_id = %execution_id,
                            line_id = id,
                            error = %e,
                            "Failed to parse log entry, skipping"
                        );
                        id += 1;
                    }
                }
            }
        }

        entries
    }

    /// Apply cursor-based pagination to a list of entries.
    ///
    /// # Direction behavior:
    /// - Forward: Returns entries with ID > cursor, ordered oldest-first
    /// - Backward: Returns entries with ID < cursor, ordered newest-first
    fn apply_pagination(
        entries: Vec<LogEntry>,
        cursor: Option<i64>,
        limit: i64,
        direction: Direction,
    ) -> PaginatedLogs {
        if entries.is_empty() {
            return PaginatedLogs::empty();
        }

        let limit = limit.max(1) as usize;

        let filtered: Vec<LogEntry> = match direction {
            Direction::Forward => {
                // Forward: get entries AFTER cursor (id > cursor)
                let start_entries: Vec<_> = match cursor {
                    Some(c) => entries.into_iter().filter(|e| e.id > c).collect(),
                    None => entries,
                };
                start_entries.into_iter().take(limit + 1).collect()
            }
            Direction::Backward => {
                // Backward: get entries BEFORE cursor (id < cursor), reversed
                let mut filtered_entries: Vec<_> = match cursor {
                    Some(c) => entries.into_iter().filter(|e| e.id < c).collect(),
                    None => entries,
                };
                // Reverse to get newest first
                filtered_entries.reverse();
                filtered_entries.into_iter().take(limit + 1).collect()
            }
        };

        let has_more = filtered.len() > limit;
        let entries_to_return: Vec<_> = filtered.into_iter().take(limit).collect();

        let next_cursor = if has_more {
            entries_to_return.last().map(|e| e.id)
        } else {
            None
        };

        PaginatedLogs::new(entries_to_return, next_cursor, has_more, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create test log records
    fn create_test_records(execution_id: Uuid, count: usize) -> Vec<ExecutionProcessLogs> {
        let mut records = Vec::new();
        let base_time = Utc::now();

        for i in 0..count {
            let logs = format!(
                r#"{{"Stdout":"message {}"}}"#,
                i + 1
            );
            let byte_size = logs.len() as i64;
            records.push(ExecutionProcessLogs {
                execution_id,
                logs,
                byte_size,
                inserted_at: base_time + chrono::Duration::milliseconds(i as i64 * 100),
            });
        }

        records
    }

    // Helper to create a record with multiple log lines
    fn create_multiline_record(execution_id: Uuid, messages: &[&str]) -> ExecutionProcessLogs {
        let logs: String = messages
            .iter()
            .map(|msg| format!(r#"{{"Stdout":"{}"}}"#, msg))
            .collect::<Vec<_>>()
            .join("\n");

        ExecutionProcessLogs {
            execution_id,
            logs,
            byte_size: 0,
            inserted_at: Utc::now(),
        }
    }

    #[test]
    fn test_parse_to_entries_single_record() {
        let execution_id = Uuid::new_v4();
        let records = create_test_records(execution_id, 1);

        let entries = ExecutionProcessLogs::parse_to_entries(&records, execution_id);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, 1);
        assert_eq!(entries[0].content, "message 1");
        assert_eq!(entries[0].execution_id, execution_id);
    }

    #[test]
    fn test_parse_to_entries_multiple_records() {
        let execution_id = Uuid::new_v4();
        let records = create_test_records(execution_id, 5);

        let entries = ExecutionProcessLogs::parse_to_entries(&records, execution_id);

        assert_eq!(entries.len(), 5);
        for (i, entry) in entries.iter().enumerate() {
            assert_eq!(entry.id, (i + 1) as i64);
            assert_eq!(entry.content, format!("message {}", i + 1));
        }
    }

    #[test]
    fn test_parse_to_entries_multiline_record() {
        let execution_id = Uuid::new_v4();
        let record = create_multiline_record(execution_id, &["first", "second", "third"]);

        let entries = ExecutionProcessLogs::parse_to_entries(&[record], execution_id);

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].content, "first");
        assert_eq!(entries[1].content, "second");
        assert_eq!(entries[2].content, "third");
        assert_eq!(entries[0].id, 1);
        assert_eq!(entries[1].id, 2);
        assert_eq!(entries[2].id, 3);
    }

    #[test]
    fn test_parse_to_entries_skips_empty_lines() {
        let execution_id = Uuid::new_v4();
        let record = ExecutionProcessLogs {
            execution_id,
            logs: r#"{"Stdout":"a"}

{"Stdout":"b"}

{"Stdout":"c"}"#
                .to_string(),
            byte_size: 0,
            inserted_at: Utc::now(),
        };

        let entries = ExecutionProcessLogs::parse_to_entries(&[record], execution_id);

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].content, "a");
        assert_eq!(entries[1].content, "b");
        assert_eq!(entries[2].content, "c");
    }

    #[test]
    fn test_parse_to_entries_handles_invalid_json() {
        let execution_id = Uuid::new_v4();
        let record = ExecutionProcessLogs {
            execution_id,
            logs: r#"{"Stdout":"valid"}
not valid json
{"Stdout":"also valid"}"#
                .to_string(),
            byte_size: 0,
            inserted_at: Utc::now(),
        };

        let entries = ExecutionProcessLogs::parse_to_entries(&[record], execution_id);

        // Should have 2 valid entries, skipping the invalid one
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].content, "valid");
        // ID increments even for skipped entries
        assert_eq!(entries[0].id, 1);
        assert_eq!(entries[1].content, "also valid");
        assert_eq!(entries[1].id, 3); // ID 2 was skipped
    }

    #[test]
    fn test_apply_pagination_forward_no_cursor() {
        let execution_id = Uuid::new_v4();
        let records = create_test_records(execution_id, 10);
        let entries = ExecutionProcessLogs::parse_to_entries(&records, execution_id);

        let result = ExecutionProcessLogs::apply_pagination(entries, None, 5, Direction::Forward);

        assert_eq!(result.entries.len(), 5);
        assert!(result.has_more);
        assert_eq!(result.next_cursor, Some(5));
        assert_eq!(result.entries[0].id, 1);
        assert_eq!(result.entries[4].id, 5);
    }

    #[test]
    fn test_apply_pagination_forward_with_cursor() {
        let execution_id = Uuid::new_v4();
        let records = create_test_records(execution_id, 10);
        let entries = ExecutionProcessLogs::parse_to_entries(&records, execution_id);

        let result = ExecutionProcessLogs::apply_pagination(entries, Some(3), 5, Direction::Forward);

        assert_eq!(result.entries.len(), 5);
        assert!(result.has_more);
        assert_eq!(result.next_cursor, Some(8));
        // Should start after cursor (id > 3)
        assert_eq!(result.entries[0].id, 4);
        assert_eq!(result.entries[4].id, 8);
    }

    #[test]
    fn test_apply_pagination_forward_last_page() {
        let execution_id = Uuid::new_v4();
        let records = create_test_records(execution_id, 10);
        let entries = ExecutionProcessLogs::parse_to_entries(&records, execution_id);

        let result = ExecutionProcessLogs::apply_pagination(entries, Some(7), 5, Direction::Forward);

        assert_eq!(result.entries.len(), 3); // Only 8, 9, 10 remain
        assert!(!result.has_more);
        assert_eq!(result.next_cursor, None);
        assert_eq!(result.entries[0].id, 8);
        assert_eq!(result.entries[2].id, 10);
    }

    #[test]
    fn test_apply_pagination_backward_no_cursor() {
        let execution_id = Uuid::new_v4();
        let records = create_test_records(execution_id, 10);
        let entries = ExecutionProcessLogs::parse_to_entries(&records, execution_id);

        let result = ExecutionProcessLogs::apply_pagination(entries, None, 5, Direction::Backward);

        assert_eq!(result.entries.len(), 5);
        assert!(result.has_more);
        assert_eq!(result.next_cursor, Some(6));
        // Backward returns newest first
        assert_eq!(result.entries[0].id, 10);
        assert_eq!(result.entries[4].id, 6);
    }

    #[test]
    fn test_apply_pagination_backward_with_cursor() {
        let execution_id = Uuid::new_v4();
        let records = create_test_records(execution_id, 10);
        let entries = ExecutionProcessLogs::parse_to_entries(&records, execution_id);

        let result =
            ExecutionProcessLogs::apply_pagination(entries, Some(8), 5, Direction::Backward);

        assert_eq!(result.entries.len(), 5);
        assert!(result.has_more);
        // Should get entries before cursor (id < 8), newest first
        assert_eq!(result.entries[0].id, 7);
        assert_eq!(result.entries[4].id, 3);
        assert_eq!(result.next_cursor, Some(3));
    }

    #[test]
    fn test_apply_pagination_backward_last_page() {
        let execution_id = Uuid::new_v4();
        let records = create_test_records(execution_id, 10);
        let entries = ExecutionProcessLogs::parse_to_entries(&records, execution_id);

        let result =
            ExecutionProcessLogs::apply_pagination(entries, Some(4), 5, Direction::Backward);

        assert_eq!(result.entries.len(), 3); // Only 1, 2, 3 remain
        assert!(!result.has_more);
        assert_eq!(result.next_cursor, None);
        assert_eq!(result.entries[0].id, 3);
        assert_eq!(result.entries[2].id, 1);
    }

    #[test]
    fn test_apply_pagination_empty_entries() {
        let result: PaginatedLogs =
            ExecutionProcessLogs::apply_pagination(vec![], None, 10, Direction::Forward);

        assert!(result.entries.is_empty());
        assert!(!result.has_more);
        assert_eq!(result.next_cursor, None);
        assert_eq!(result.total_count, Some(0));
    }

    #[test]
    fn test_apply_pagination_exact_limit() {
        let execution_id = Uuid::new_v4();
        let records = create_test_records(execution_id, 5);
        let entries = ExecutionProcessLogs::parse_to_entries(&records, execution_id);

        let result = ExecutionProcessLogs::apply_pagination(entries, None, 5, Direction::Forward);

        assert_eq!(result.entries.len(), 5);
        assert!(!result.has_more);
        assert_eq!(result.next_cursor, None);
    }

    #[test]
    fn test_apply_pagination_limit_exceeds_entries() {
        let execution_id = Uuid::new_v4();
        let records = create_test_records(execution_id, 3);
        let entries = ExecutionProcessLogs::parse_to_entries(&records, execution_id);

        let result = ExecutionProcessLogs::apply_pagination(entries, None, 10, Direction::Forward);

        assert_eq!(result.entries.len(), 3);
        assert!(!result.has_more);
        assert_eq!(result.next_cursor, None);
    }

    #[test]
    fn test_apply_pagination_cursor_beyond_entries() {
        let execution_id = Uuid::new_v4();
        let records = create_test_records(execution_id, 5);
        let entries = ExecutionProcessLogs::parse_to_entries(&records, execution_id);

        let result =
            ExecutionProcessLogs::apply_pagination(entries, Some(100), 10, Direction::Forward);

        // No entries after cursor 100
        assert!(result.entries.is_empty());
        assert!(!result.has_more);
    }

    #[test]
    fn test_apply_pagination_backward_cursor_at_start() {
        let execution_id = Uuid::new_v4();
        let records = create_test_records(execution_id, 5);
        let entries = ExecutionProcessLogs::parse_to_entries(&records, execution_id);

        let result =
            ExecutionProcessLogs::apply_pagination(entries, Some(1), 10, Direction::Backward);

        // No entries before cursor 1
        assert!(result.entries.is_empty());
        assert!(!result.has_more);
    }

    #[test]
    fn test_with_total_count() {
        let paginated = PaginatedLogs::empty().with_total_count(42);

        assert_eq!(paginated.total_count, Some(42));
    }

    #[test]
    fn test_parse_to_entries_preserves_timestamp() {
        let execution_id = Uuid::new_v4();
        let timestamp = Utc::now();
        let record = ExecutionProcessLogs {
            execution_id,
            logs: r#"{"Stdout":"test"}"#.to_string(),
            byte_size: 0,
            inserted_at: timestamp,
        };

        let entries = ExecutionProcessLogs::parse_to_entries(&[record], execution_id);

        assert_eq!(entries[0].timestamp, timestamp);
    }

    #[test]
    fn test_parse_to_entries_stderr() {
        let execution_id = Uuid::new_v4();
        let record = ExecutionProcessLogs {
            execution_id,
            logs: r#"{"Stderr":"error message"}"#.to_string(),
            byte_size: 0,
            inserted_at: Utc::now(),
        };

        let entries = ExecutionProcessLogs::parse_to_entries(&[record], execution_id);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content, "error message");
        assert_eq!(
            entries[0].output_type,
            utils::unified_log::OutputType::Stderr
        );
    }

    #[test]
    fn test_parse_to_entries_mixed_types() {
        let execution_id = Uuid::new_v4();
        let record = ExecutionProcessLogs {
            execution_id,
            logs: r#"{"Stdout":"out"}
{"Stderr":"err"}
{"SessionId":"sess123"}
"Finished""#
                .to_string(),
            byte_size: 0,
            inserted_at: Utc::now(),
        };

        let entries = ExecutionProcessLogs::parse_to_entries(&[record], execution_id);

        assert_eq!(entries.len(), 4);
        assert_eq!(
            entries[0].output_type,
            utils::unified_log::OutputType::Stdout
        );
        assert_eq!(
            entries[1].output_type,
            utils::unified_log::OutputType::Stderr
        );
        assert_eq!(
            entries[2].output_type,
            utils::unified_log::OutputType::SessionId
        );
        assert_eq!(
            entries[3].output_type,
            utils::unified_log::OutputType::Finished
        );
    }
}
