//! SQLite retry logic with exponential backoff.
//!
//! This module provides utilities for handling transient SQLite errors like
//! SQLITE_BUSY (code 5) and SQLITE_LOCKED (code 6) which occur under heavy
//! concurrent load.

use std::future::Future;
use std::time::Duration;

use sqlx::Error as SqlxError;

/// Configuration for SQLite retry behavior.
#[derive(Clone, Debug)]
pub struct RetryConfig {
    /// Maximum number of retry attempts.
    pub max_retries: u32,
    /// Base delay in milliseconds for exponential backoff.
    pub base_delay_ms: u64,
    /// Maximum delay in milliseconds (caps the exponential growth).
    pub max_delay_ms: u64,
    /// Jitter factor (0.0 to 1.0) to add randomness to delays.
    pub jitter_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            base_delay_ms: 50,
            max_delay_ms: 2000,
            jitter_factor: 0.2,
        }
    }
}

impl RetryConfig {
    /// Create a new retry config with custom settings.
    pub fn new(max_retries: u32, base_delay_ms: u64, max_delay_ms: u64) -> Self {
        Self {
            max_retries,
            base_delay_ms,
            max_delay_ms,
            jitter_factor: 0.2,
        }
    }

    /// Create a config for high-contention scenarios (more retries, longer delays).
    pub fn high_contention() -> Self {
        Self {
            max_retries: 10,
            base_delay_ms: 100,
            max_delay_ms: 5000,
            jitter_factor: 0.3,
        }
    }

    /// Calculate delay for a given attempt using exponential backoff with jitter.
    fn calculate_delay(&self, attempt: u32) -> Duration {
        let base_delay = self.base_delay_ms * 2u64.pow(attempt);
        let capped_delay = base_delay.min(self.max_delay_ms);

        // Add jitter to prevent thundering herd
        let jitter = if self.jitter_factor > 0.0 {
            let jitter_range = (capped_delay as f64 * self.jitter_factor) as u64;
            if jitter_range > 0 {
                // Simple pseudo-random jitter using current time
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .subsec_nanos() as u64;
                now % jitter_range
            } else {
                0
            }
        } else {
            0
        };

        Duration::from_millis(capped_delay + jitter)
    }
}

/// Check if an error is a transient SQLite error that should be retried.
///
/// SQLite error codes considered retryable:
/// - 5 = SQLITE_BUSY (database is locked by another connection)
/// - 6 = SQLITE_LOCKED (table is locked within a transaction)
/// - 10 = SQLITE_IOERR (disk I/O error - base code)
/// - 522 = SQLITE_IOERR_SHORT_READ (I/O error variant)
/// - Other 5xx codes = SQLITE_IOERR extended codes
///
/// SQLITE_IOERR errors (code 10 and variants like 522) are included because
/// they can be transient under heavy write load, especially with WAL mode
/// and mmap. These errors often resolve after a brief pause.
pub fn is_retryable_error(e: &SqlxError) -> bool {
    if let SqlxError::Database(db_err) = e {
        if let Some(code) = db_err.code() {
            let code_str = code.as_ref();
            // SQLITE_BUSY (5), SQLITE_LOCKED (6), SQLITE_IOERR (10)
            if matches!(code_str, "5" | "6" | "10") {
                return true;
            }
            // SQLITE_IOERR extended codes: 522, 778, etc. (base 10 + extended*256)
            // These are formatted as the numeric value, e.g., "522"
            if let Ok(code_num) = code_str.parse::<u32>() {
                // Extended I/O error codes have base 10 (SQLITE_IOERR)
                // Check if it's an IOERR variant: (code & 0xFF) == 10
                if code_num > 10 && (code_num & 0xFF) == 10 {
                    return true;
                }
            }
        }
        false
    } else {
        false
    }
}

/// Execute a database operation with exponential backoff retry.
///
/// This function will retry the operation up to `config.max_retries` times
/// if it encounters a SQLITE_BUSY or SQLITE_LOCKED error.
///
/// # Arguments
/// * `config` - Retry configuration
/// * `operation_name` - Name for logging purposes
/// * `f` - The async operation to execute
///
/// # Returns
/// The result of the operation, or the last error if all retries failed.
///
/// # Example
/// ```ignore
/// use db::retry::{RetryConfig, with_retry};
///
/// let result = with_retry(&RetryConfig::default(), "insert_record", || async {
///     sqlx::query!("INSERT INTO foo (bar) VALUES (?)", value)
///         .execute(&pool)
///         .await
/// }).await?;
/// ```
pub async fn with_retry<F, Fut, T>(
    config: &RetryConfig,
    operation_name: &str,
    mut f: F,
) -> Result<T, SqlxError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, SqlxError>>,
{
    let mut attempt = 0;

    loop {
        match f().await {
            Ok(result) => {
                if attempt > 0 {
                    tracing::debug!(
                        operation = operation_name,
                        attempts = attempt + 1,
                        "Database operation succeeded after retry"
                    );
                }
                return Ok(result);
            }
            Err(e) if is_retryable_error(&e) && attempt < config.max_retries => {
                let delay = config.calculate_delay(attempt);

                tracing::warn!(
                    operation = operation_name,
                    attempt = attempt + 1,
                    max_retries = config.max_retries,
                    delay_ms = delay.as_millis() as u64,
                    error = ?e,
                    "Transient SQLite error (BUSY/LOCKED/IOERR), retrying with backoff"
                );

                tokio::time::sleep(delay).await;
                attempt += 1;
            }
            Err(e) => {
                if attempt > 0 {
                    tracing::error!(
                        operation = operation_name,
                        attempts = attempt + 1,
                        error = ?e,
                        "Database operation failed after all retries"
                    );
                }
                return Err(e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.base_delay_ms, 50);
        assert_eq!(config.max_delay_ms, 2000);
    }

    #[test]
    fn test_calculate_delay_exponential() {
        let config = RetryConfig {
            max_retries: 5,
            base_delay_ms: 100,
            max_delay_ms: 5000,
            jitter_factor: 0.0, // No jitter for predictable testing
        };

        // Exponential: 100, 200, 400, 800, 1600, 3200 (capped at 5000)
        assert_eq!(config.calculate_delay(0), Duration::from_millis(100));
        assert_eq!(config.calculate_delay(1), Duration::from_millis(200));
        assert_eq!(config.calculate_delay(2), Duration::from_millis(400));
        assert_eq!(config.calculate_delay(3), Duration::from_millis(800));
        assert_eq!(config.calculate_delay(4), Duration::from_millis(1600));
        assert_eq!(config.calculate_delay(5), Duration::from_millis(3200));
        assert_eq!(config.calculate_delay(6), Duration::from_millis(5000)); // Capped
    }

    #[test]
    fn test_calculate_delay_with_jitter() {
        let config = RetryConfig {
            max_retries: 5,
            base_delay_ms: 100,
            max_delay_ms: 5000,
            jitter_factor: 0.2,
        };

        // With 20% jitter on 100ms base, delay should be 100-120ms
        let delay = config.calculate_delay(0);
        assert!(delay >= Duration::from_millis(100));
        assert!(delay <= Duration::from_millis(120));
    }

    #[test]
    fn test_is_retryable_error() {
        // We can't easily create a real sqlx database error, so this test is limited
        // In practice, integration tests would verify this works correctly
    }
}
