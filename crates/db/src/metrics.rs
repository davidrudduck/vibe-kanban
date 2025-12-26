//! Database metrics collection for monitoring SQLite performance.
//!
//! This module provides atomic counters and histogram-like tracking for
//! database operations, enabling visibility into query performance,
//! connection pool health, and retry behavior.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use sqlx::SqlitePool;
use ts_rs::TS;

/// Database metrics collector.
///
/// All metrics are tracked using atomic operations for thread-safety
/// without locking overhead.
#[derive(Clone, Default)]
pub struct DbMetrics {
    inner: Arc<DbMetricsInner>,
}

#[derive(Default)]
struct DbMetricsInner {
    // Query timing
    queries_total: AtomicU64,
    queries_slow: AtomicU64, // > slow_query_threshold
    query_duration_sum_us: AtomicU64,

    // Retry metrics
    retry_attempts: AtomicU64,
    retry_successes: AtomicU64,
    retry_failures: AtomicU64,

    // Connection pool
    pool_acquires_total: AtomicU64,
    pool_acquire_wait_us: AtomicU64,
    pool_timeouts: AtomicU64,

    // Error tracking
    busy_errors: AtomicU64,
    other_errors: AtomicU64,

    // WAL monitoring (updated by WalMonitor)
    wal_size_bytes: AtomicU64,
    last_checkpoint_duration_us: AtomicU64,

    // Latency buckets for histogram approximation
    // Buckets: <1ms, <5ms, <10ms, <50ms, <100ms, <500ms, <1s, >=1s
    latency_bucket_1ms: AtomicU64,
    latency_bucket_5ms: AtomicU64,
    latency_bucket_10ms: AtomicU64,
    latency_bucket_50ms: AtomicU64,
    latency_bucket_100ms: AtomicU64,
    latency_bucket_500ms: AtomicU64,
    latency_bucket_1s: AtomicU64,
    latency_bucket_inf: AtomicU64,
}

/// Threshold in milliseconds for considering a query "slow".
fn slow_query_threshold_ms() -> u64 {
    std::env::var("VK_SLOW_QUERY_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100)
}

impl DbMetrics {
    /// Create a new metrics collector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a query execution with its duration.
    pub fn record_query(&self, duration: Duration) {
        let us = duration.as_micros() as u64;
        let ms = duration.as_millis() as u64;

        self.inner.queries_total.fetch_add(1, Ordering::Relaxed);
        self.inner
            .query_duration_sum_us
            .fetch_add(us, Ordering::Relaxed);

        // Track slow queries
        if ms > slow_query_threshold_ms() {
            self.inner.queries_slow.fetch_add(1, Ordering::Relaxed);
        }

        // Update latency histogram bucket
        self.record_latency_bucket(ms);
    }

    fn record_latency_bucket(&self, ms: u64) {
        let bucket = if ms < 1 {
            &self.inner.latency_bucket_1ms
        } else if ms < 5 {
            &self.inner.latency_bucket_5ms
        } else if ms < 10 {
            &self.inner.latency_bucket_10ms
        } else if ms < 50 {
            &self.inner.latency_bucket_50ms
        } else if ms < 100 {
            &self.inner.latency_bucket_100ms
        } else if ms < 500 {
            &self.inner.latency_bucket_500ms
        } else if ms < 1000 {
            &self.inner.latency_bucket_1s
        } else {
            &self.inner.latency_bucket_inf
        };
        bucket.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a retry attempt and whether it succeeded.
    pub fn record_retry(&self, succeeded: bool) {
        self.inner.retry_attempts.fetch_add(1, Ordering::Relaxed);
        if succeeded {
            self.inner.retry_successes.fetch_add(1, Ordering::Relaxed);
        } else {
            self.inner.retry_failures.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record a SQLITE_BUSY error.
    pub fn record_busy_error(&self) {
        self.inner.busy_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a non-BUSY database error.
    pub fn record_other_error(&self) {
        self.inner.other_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a connection pool acquisition with wait time.
    pub fn record_pool_acquire(&self, wait_duration: Duration) {
        self.inner.pool_acquires_total.fetch_add(1, Ordering::Relaxed);
        self.inner.pool_acquire_wait_us.fetch_add(
            wait_duration.as_micros() as u64,
            Ordering::Relaxed,
        );
    }

    /// Record a pool acquisition timeout.
    pub fn record_pool_timeout(&self) {
        self.inner.pool_timeouts.fetch_add(1, Ordering::Relaxed);
    }

    /// Update WAL file size (called by WalMonitor).
    pub fn update_wal_size(&self, bytes: u64) {
        self.inner.wal_size_bytes.store(bytes, Ordering::Relaxed);
    }

    /// Update last checkpoint duration (called by maintenance service).
    pub fn update_checkpoint_duration(&self, duration: Duration) {
        self.inner
            .last_checkpoint_duration_us
            .store(duration.as_micros() as u64, Ordering::Relaxed);
    }

    /// Get current pool statistics from SQLx.
    pub fn get_pool_stats(&self, pool: &SqlitePool) -> PoolStats {
        PoolStats {
            size: pool.size(),
            idle: pool.num_idle() as u32,
            acquired: pool.size().saturating_sub(pool.num_idle() as u32),
        }
    }

    /// Get a snapshot of all metrics.
    pub fn snapshot(&self) -> DbMetricsSnapshot {
        let total_queries = self.inner.queries_total.load(Ordering::Relaxed);
        let total_duration_us = self.inner.query_duration_sum_us.load(Ordering::Relaxed);

        DbMetricsSnapshot {
            queries_total: total_queries,
            queries_slow: self.inner.queries_slow.load(Ordering::Relaxed),
            query_avg_duration_ms: if total_queries > 0 {
                (total_duration_us as f64 / total_queries as f64) / 1000.0
            } else {
                0.0
            },
            retry_attempts: self.inner.retry_attempts.load(Ordering::Relaxed),
            retry_successes: self.inner.retry_successes.load(Ordering::Relaxed),
            retry_failures: self.inner.retry_failures.load(Ordering::Relaxed),
            busy_errors: self.inner.busy_errors.load(Ordering::Relaxed),
            other_errors: self.inner.other_errors.load(Ordering::Relaxed),
            pool_acquires_total: self.inner.pool_acquires_total.load(Ordering::Relaxed),
            pool_timeouts: self.inner.pool_timeouts.load(Ordering::Relaxed),
            wal_size_bytes: self.inner.wal_size_bytes.load(Ordering::Relaxed),
            last_checkpoint_duration_ms: self.inner.last_checkpoint_duration_us.load(Ordering::Relaxed)
                / 1000,
            latency_p50_ms: self.estimate_percentile(50),
            latency_p95_ms: self.estimate_percentile(95),
            latency_p99_ms: self.estimate_percentile(99),
        }
    }

    /// Estimate a percentile from the histogram buckets.
    ///
    /// This is an approximation since we use fixed buckets rather than
    /// storing all values.
    fn estimate_percentile(&self, percentile: u64) -> u64 {
        let buckets = [
            (1, self.inner.latency_bucket_1ms.load(Ordering::Relaxed)),
            (5, self.inner.latency_bucket_5ms.load(Ordering::Relaxed)),
            (10, self.inner.latency_bucket_10ms.load(Ordering::Relaxed)),
            (50, self.inner.latency_bucket_50ms.load(Ordering::Relaxed)),
            (100, self.inner.latency_bucket_100ms.load(Ordering::Relaxed)),
            (500, self.inner.latency_bucket_500ms.load(Ordering::Relaxed)),
            (1000, self.inner.latency_bucket_1s.load(Ordering::Relaxed)),
            (5000, self.inner.latency_bucket_inf.load(Ordering::Relaxed)), // Assume 5s for overflow
        ];

        let total: u64 = buckets.iter().map(|(_, count)| count).sum();
        if total == 0 {
            return 0;
        }

        let target = (total * percentile) / 100;
        let mut cumulative = 0u64;

        for (upper_bound, count) in buckets {
            cumulative += count;
            if cumulative >= target {
                return upper_bound;
            }
        }

        buckets.last().map(|(b, _)| *b).unwrap_or(0)
    }
}

/// Connection pool statistics.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct PoolStats {
    /// Total connections in the pool.
    pub size: u32,
    /// Idle connections available.
    pub idle: u32,
    /// Currently acquired connections.
    pub acquired: u32,
}

/// Snapshot of all database metrics.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DbMetricsSnapshot {
    /// Total number of queries executed.
    pub queries_total: u64,
    /// Number of slow queries (> threshold).
    pub queries_slow: u64,
    /// Average query duration in milliseconds.
    pub query_avg_duration_ms: f64,
    /// Total retry attempts due to SQLITE_BUSY.
    pub retry_attempts: u64,
    /// Retries that succeeded.
    pub retry_successes: u64,
    /// Retries that failed after all attempts.
    pub retry_failures: u64,
    /// SQLITE_BUSY errors encountered.
    pub busy_errors: u64,
    /// Other database errors.
    pub other_errors: u64,
    /// Total pool connection acquisitions.
    pub pool_acquires_total: u64,
    /// Pool acquisition timeouts.
    pub pool_timeouts: u64,
    /// Current WAL file size in bytes.
    pub wal_size_bytes: u64,
    /// Last checkpoint duration in milliseconds.
    pub last_checkpoint_duration_ms: u64,
    /// Estimated 50th percentile latency (ms).
    pub latency_p50_ms: u64,
    /// Estimated 95th percentile latency (ms).
    pub latency_p95_ms: u64,
    /// Estimated 99th percentile latency (ms).
    pub latency_p99_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_new() {
        let metrics = DbMetrics::new();
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.queries_total, 0);
        assert_eq!(snapshot.retry_attempts, 0);
    }

    #[test]
    fn test_record_query() {
        let metrics = DbMetrics::new();

        metrics.record_query(Duration::from_millis(10));
        metrics.record_query(Duration::from_millis(20));
        metrics.record_query(Duration::from_millis(30));

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.queries_total, 3);
        assert_eq!(snapshot.queries_slow, 0); // All under 100ms
    }

    #[test]
    fn test_record_slow_query() {
        let metrics = DbMetrics::new();

        metrics.record_query(Duration::from_millis(50));
        metrics.record_query(Duration::from_millis(150)); // Slow
        metrics.record_query(Duration::from_millis(200)); // Slow

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.queries_total, 3);
        assert_eq!(snapshot.queries_slow, 2);
    }

    #[test]
    fn test_record_retry() {
        let metrics = DbMetrics::new();

        metrics.record_retry(true);
        metrics.record_retry(true);
        metrics.record_retry(false);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.retry_attempts, 3);
        assert_eq!(snapshot.retry_successes, 2);
        assert_eq!(snapshot.retry_failures, 1);
    }

    #[test]
    fn test_wal_size_update() {
        let metrics = DbMetrics::new();

        metrics.update_wal_size(1024 * 1024); // 1MB

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.wal_size_bytes, 1024 * 1024);
    }

    #[test]
    fn test_latency_percentile_estimate() {
        let metrics = DbMetrics::new();

        // Record 100 queries in the 10ms bucket
        for _ in 0..100 {
            metrics.record_query(Duration::from_millis(8));
        }

        let snapshot = metrics.snapshot();
        // All queries are in the <10ms bucket, so p50/p95/p99 should be 10
        assert_eq!(snapshot.latency_p50_ms, 10);
        assert_eq!(snapshot.latency_p95_ms, 10);
        assert_eq!(snapshot.latency_p99_ms, 10);
    }
}
