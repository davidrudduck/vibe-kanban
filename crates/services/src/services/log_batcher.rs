//! Log batching service for efficient database writes.
//!
//! This module provides a batching layer over individual log writes to prevent
//! overwhelming SQLite under heavy concurrent load. Instead of writing each
//! log line individually, messages are buffered and flushed in batches.
//!
//! # Design
//!
//! - Messages are buffered per execution_id
//! - Flush occurs when:
//!   - Buffer reaches `BATCH_SIZE` messages (default: 100)
//!   - `FLUSH_INTERVAL_MS` elapsed since last flush (default: 250ms)
//!   - Execution completes (explicit finish signal)
//!
//! # Benefits
//!
//! - Reduces individual INSERT transactions by ~100x under heavy load
//! - Uses retry logic for transient SQLITE_BUSY errors
//! - Maintains ordering within each execution

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use db::models::execution_process_logs::ExecutionProcessLogs;
use db::retry::{RetryConfig, with_retry};
use db::DBService;
use sqlx::SqlitePool;
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tokio::time::interval;
use utils::log_msg::LogMsg;
use uuid::Uuid;

/// Maximum messages to buffer before forced flush.
const BATCH_SIZE: usize = 100;

/// Flush interval in milliseconds (flushes all buffers periodically).
const FLUSH_INTERVAL_MS: u64 = 250;

/// Channel buffer size per-batcher.
const CHANNEL_BUFFER: usize = 10000;

/// Handle for sending log messages to the batcher.
#[derive(Clone)]
pub struct LogBatcherHandle {
    tx: mpsc::Sender<LogBatcherCommand>,
}

enum LogBatcherCommand {
    /// Add a log message for an execution.
    AddLog { execution_id: Uuid, msg: LogMsg },
    /// Signal that an execution has finished (flush remaining logs).
    Finish { execution_id: Uuid },
    /// Shutdown the batcher.
    Shutdown,
}

impl LogBatcherHandle {
    /// Add a log message to be batched.
    ///
    /// This is non-blocking and will buffer the message for later writing.
    pub async fn add_log(&self, execution_id: Uuid, msg: LogMsg) {
        if let Err(e) = self
            .tx
            .send(LogBatcherCommand::AddLog { execution_id, msg })
            .await
        {
            tracing::error!("Failed to send log to batcher: {}", e);
        }
    }

    /// Signal that an execution has finished.
    ///
    /// This flushes any remaining buffered logs for this execution.
    pub async fn finish(&self, execution_id: Uuid) {
        if let Err(e) = self
            .tx
            .send(LogBatcherCommand::Finish { execution_id })
            .await
        {
            tracing::error!(
                "Failed to send finish signal to batcher for {}: {}",
                execution_id,
                e
            );
        }
    }

    /// Shutdown the batcher gracefully.
    pub async fn shutdown(&self) {
        let _ = self.tx.send(LogBatcherCommand::Shutdown).await;
    }
}

/// Log batcher service that buffers and batches database writes.
pub struct LogBatcher {
    pool: SqlitePool,
    buffers: Arc<RwLock<HashMap<Uuid, Vec<String>>>>,
    retry_config: RetryConfig,
}

impl LogBatcher {
    /// Spawn a new log batcher service.
    ///
    /// Returns a handle that can be cloned and used to send log messages.
    pub fn spawn(db: &DBService) -> LogBatcherHandle {
        let (tx, rx) = mpsc::channel(CHANNEL_BUFFER);
        let batcher = Self {
            pool: db.pool.clone(),
            buffers: Arc::new(RwLock::new(HashMap::new())),
            retry_config: RetryConfig::default(),
        };
        tokio::spawn(batcher.run(rx));
        LogBatcherHandle { tx }
    }

    async fn run(self, mut rx: mpsc::Receiver<LogBatcherCommand>) {
        let mut flush_interval = interval(Duration::from_millis(FLUSH_INTERVAL_MS));

        loop {
            tokio::select! {
                Some(cmd) = rx.recv() => {
                    match cmd {
                        LogBatcherCommand::AddLog { execution_id, msg } => {
                            self.buffer_log(execution_id, msg).await;
                        }
                        LogBatcherCommand::Finish { execution_id } => {
                            self.flush_execution(execution_id).await;
                        }
                        LogBatcherCommand::Shutdown => {
                            // Flush all remaining buffers before shutdown
                            self.flush_all().await;
                            break;
                        }
                    }
                }
                _ = flush_interval.tick() => {
                    self.flush_all_if_threshold().await;
                }
                else => break,
            }
        }

        tracing::info!("Log batcher shutting down");
    }

    /// Buffer a log message, flushing if batch size is reached.
    async fn buffer_log(&self, execution_id: Uuid, msg: LogMsg) {
        // Only buffer Stdout, Stderr, and JsonPatch messages
        if !matches!(
            msg,
            LogMsg::Stdout(_) | LogMsg::Stderr(_) | LogMsg::JsonPatch(_)
        ) {
            return;
        }

        let jsonl_line = match serde_json::to_string(&msg) {
            Ok(line) => format!("{line}\n"),
            Err(e) => {
                tracing::error!(
                    execution_id = %execution_id,
                    error = %e,
                    "Failed to serialize log message"
                );
                return;
            }
        };

        let should_flush = {
            let mut buffers = self.buffers.write().await;
            let buffer = buffers.entry(execution_id).or_default();
            buffer.push(jsonl_line);
            buffer.len() >= BATCH_SIZE
        };

        if should_flush {
            self.flush_execution(execution_id).await;
        }
    }

    /// Flush all buffered logs for a specific execution.
    async fn flush_execution(&self, execution_id: Uuid) {
        let lines = {
            let mut buffers = self.buffers.write().await;
            buffers.remove(&execution_id).unwrap_or_default()
        };

        if lines.is_empty() {
            return;
        }

        let line_count = lines.len();
        let batch_content = lines.join("");

        if let Err(e) = self.insert_with_retry(execution_id, &batch_content).await {
            tracing::error!(
                execution_id = %execution_id,
                line_count = line_count,
                error = ?e,
                "Failed to batch insert logs after retries"
            );
        } else {
            tracing::debug!(
                execution_id = %execution_id,
                line_count = line_count,
                "Flushed log batch to database"
            );
        }
    }

    /// Insert batch with retry logic for SQLITE_BUSY errors.
    async fn insert_with_retry(
        &self,
        execution_id: Uuid,
        content: &str,
    ) -> Result<(), sqlx::Error> {
        let pool = self.pool.clone();
        let content = content.to_string();

        with_retry(&self.retry_config, "batch_insert_logs", || {
            let pool = pool.clone();
            let content = content.clone();
            async move {
                ExecutionProcessLogs::append_log_line(&pool, execution_id, &content).await
            }
        })
        .await
    }

    /// Flush all buffers that have been waiting.
    async fn flush_all_if_threshold(&self) {
        // Get all execution IDs with pending logs
        let execution_ids: Vec<Uuid> = {
            let buffers = self.buffers.read().await;
            buffers
                .iter()
                .filter(|(_, buf)| !buf.is_empty())
                .map(|(id, _)| *id)
                .collect()
        };

        // Flush each one
        for execution_id in execution_ids {
            self.flush_execution(execution_id).await;
        }
    }

    /// Flush all buffers (used during shutdown).
    async fn flush_all(&self) {
        let execution_ids: Vec<Uuid> = {
            let buffers = self.buffers.read().await;
            buffers.keys().cloned().collect()
        };

        for execution_id in execution_ids {
            self.flush_execution(execution_id).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_size_constant() {
        assert_eq!(BATCH_SIZE, 100);
    }

    #[test]
    fn test_flush_interval_constant() {
        assert_eq!(FLUSH_INTERVAL_MS, 250);
    }
}
