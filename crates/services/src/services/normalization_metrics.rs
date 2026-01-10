//! Normalization metrics collection for monitoring log normalization performance.
//!
//! This module tracks normalization completion times and timeout frequency,
//! enabling visibility into executor log processing health.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;
use ts_rs::TS;

/// Normalization metrics collector.
///
/// All metrics are tracked using atomic operations for thread-safety
/// without locking overhead.
#[derive(Clone, Default)]
pub struct NormalizationMetrics {
    inner: Arc<NormalizationMetricsInner>,
}

#[derive(Default)]
struct NormalizationMetricsInner {
    /// Total normalization completions (includes timeouts)
    total: AtomicU64,
    /// Number of normalization timeouts
    timeouts: AtomicU64,
    /// Sum of all completion durations in microseconds
    duration_sum_us: AtomicU64,

    // Latency buckets for histogram approximation
    // Buckets: <100ms, <500ms, <1s, <2s, <5s, >=5s
    latency_under_100ms: AtomicU64,
    latency_under_500ms: AtomicU64,
    latency_under_1s: AtomicU64,
    latency_under_2s: AtomicU64,
    latency_under_5s: AtomicU64,
    latency_over_5s: AtomicU64,
}

impl NormalizationMetrics {
    /// Create a new metrics collector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a successful normalization completion with its duration.
    pub fn record_completion(&self, duration: Duration) {
        self.inner.total.fetch_add(1, Ordering::Relaxed);
        let us = duration.as_micros() as u64;
        self.inner.duration_sum_us.fetch_add(us, Ordering::Relaxed);

        let ms = duration.as_millis() as u64;
        self.record_latency_bucket(ms);
    }

    /// Record a normalization timeout.
    pub fn record_timeout(&self) {
        self.inner.total.fetch_add(1, Ordering::Relaxed);
        self.inner.timeouts.fetch_add(1, Ordering::Relaxed);
    }

    fn record_latency_bucket(&self, ms: u64) {
        let bucket = match ms {
            0..=100 => &self.inner.latency_under_100ms,
            101..=500 => &self.inner.latency_under_500ms,
            501..=1000 => &self.inner.latency_under_1s,
            1001..=2000 => &self.inner.latency_under_2s,
            2001..=5000 => &self.inner.latency_under_5s,
            _ => &self.inner.latency_over_5s,
        };
        bucket.fetch_add(1, Ordering::Relaxed);
    }

    /// Get a snapshot of all metrics.
    pub fn snapshot(&self) -> NormalizationMetricsSnapshot {
        let total = self.inner.total.load(Ordering::Relaxed);
        let timeouts = self.inner.timeouts.load(Ordering::Relaxed);
        let duration_sum_us = self.inner.duration_sum_us.load(Ordering::Relaxed);

        // Successful completions = total - timeouts
        let completions = total.saturating_sub(timeouts);

        NormalizationMetricsSnapshot {
            total,
            timeouts,
            timeout_rate: if total == 0 {
                0.0
            } else {
                timeouts as f64 / total as f64
            },
            avg_duration_ms: if completions == 0 {
                0.0
            } else {
                (duration_sum_us as f64 / 1000.0) / completions as f64
            },
            latency_buckets: LatencyBuckets {
                under_100ms: self.inner.latency_under_100ms.load(Ordering::Relaxed),
                under_500ms: self.inner.latency_under_500ms.load(Ordering::Relaxed),
                under_1s: self.inner.latency_under_1s.load(Ordering::Relaxed),
                under_2s: self.inner.latency_under_2s.load(Ordering::Relaxed),
                under_5s: self.inner.latency_under_5s.load(Ordering::Relaxed),
                over_5s: self.inner.latency_over_5s.load(Ordering::Relaxed),
            },
        }
    }

    /// Spawn a background task to log metrics periodically (every 5 minutes).
    ///
    /// Returns a JoinHandle that can be used to cancel the task.
    pub fn spawn_periodic_logger(&self) -> tokio::task::JoinHandle<()> {
        let metrics = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300));
            loop {
                interval.tick().await;
                let snapshot = metrics.snapshot();
                if snapshot.total > 0 {
                    tracing::info!(
                        total = snapshot.total,
                        timeouts = snapshot.timeouts,
                        timeout_rate_pct = format!("{:.2}", snapshot.timeout_rate * 100.0),
                        avg_duration_ms = format!("{:.2}", snapshot.avg_duration_ms),
                        "Normalization metrics"
                    );
                }
            }
        })
    }
}

/// Latency distribution buckets for normalization durations.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct LatencyBuckets {
    /// Completions under 100ms
    pub under_100ms: u64,
    /// Completions between 100ms and 500ms
    pub under_500ms: u64,
    /// Completions between 500ms and 1s
    pub under_1s: u64,
    /// Completions between 1s and 2s
    pub under_2s: u64,
    /// Completions between 2s and 5s
    pub under_5s: u64,
    /// Completions over 5s (or timeouts)
    pub over_5s: u64,
}

/// Snapshot of normalization metrics.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct NormalizationMetricsSnapshot {
    /// Total normalization attempts (completions + timeouts)
    pub total: u64,
    /// Number of normalization timeouts
    pub timeouts: u64,
    /// Timeout rate (0.0 to 1.0)
    pub timeout_rate: f64,
    /// Average duration in milliseconds for successful completions
    pub avg_duration_ms: f64,
    /// Latency distribution buckets
    pub latency_buckets: LatencyBuckets,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_completion_increments_total() {
        let metrics = NormalizationMetrics::new();
        metrics.record_completion(Duration::from_millis(100));
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.total, 1);
        assert_eq!(snapshot.timeouts, 0);
    }

    #[test]
    fn test_record_timeout_increments_both() {
        let metrics = NormalizationMetrics::new();
        metrics.record_timeout();
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.total, 1);
        assert_eq!(snapshot.timeouts, 1);
    }

    #[test]
    fn test_latency_buckets() {
        let metrics = NormalizationMetrics::new();
        metrics.record_completion(Duration::from_millis(50)); // <100ms
        metrics.record_completion(Duration::from_millis(200)); // <500ms
        metrics.record_completion(Duration::from_millis(6000)); // >5s
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.latency_buckets.under_100ms, 1);
        assert_eq!(snapshot.latency_buckets.under_500ms, 1);
        assert_eq!(snapshot.latency_buckets.over_5s, 1);
    }

    #[test]
    fn test_timeout_rate_calculation() {
        let metrics = NormalizationMetrics::new();
        metrics.record_completion(Duration::from_millis(100));
        metrics.record_timeout();
        let snapshot = metrics.snapshot();
        assert!((snapshot.timeout_rate - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_avg_duration_excludes_timeouts() {
        let metrics = NormalizationMetrics::new();
        // Record two completions: 100ms and 200ms
        metrics.record_completion(Duration::from_millis(100));
        metrics.record_completion(Duration::from_millis(200));
        // Record a timeout (should not affect avg_duration_ms)
        metrics.record_timeout();

        let snapshot = metrics.snapshot();
        // Average of 100 and 200 = 150ms
        assert!((snapshot.avg_duration_ms - 150.0).abs() < 1.0);
    }

    #[test]
    fn test_empty_metrics() {
        let metrics = NormalizationMetrics::new();
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.total, 0);
        assert_eq!(snapshot.timeouts, 0);
        assert_eq!(snapshot.timeout_rate, 0.0);
        assert_eq!(snapshot.avg_duration_ms, 0.0);
    }

    #[test]
    fn test_all_latency_buckets() {
        let metrics = NormalizationMetrics::new();
        metrics.record_completion(Duration::from_millis(50)); // <100ms
        metrics.record_completion(Duration::from_millis(300)); // 100-500ms
        metrics.record_completion(Duration::from_millis(750)); // 500-1000ms
        metrics.record_completion(Duration::from_millis(1500)); // 1-2s
        metrics.record_completion(Duration::from_millis(3500)); // 2-5s
        metrics.record_completion(Duration::from_millis(7000)); // >5s

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.latency_buckets.under_100ms, 1);
        assert_eq!(snapshot.latency_buckets.under_500ms, 1);
        assert_eq!(snapshot.latency_buckets.under_1s, 1);
        assert_eq!(snapshot.latency_buckets.under_2s, 1);
        assert_eq!(snapshot.latency_buckets.under_5s, 1);
        assert_eq!(snapshot.latency_buckets.over_5s, 1);
        assert_eq!(snapshot.total, 6);
    }
}
