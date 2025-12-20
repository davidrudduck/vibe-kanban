//! In-memory cache for paginated log responses.
//!
//! This cache reduces database load by caching recent log pagination results
//! with a short TTL (200ms). The cache is keyed by (assignment_id, cursor, limit, direction).

use dashmap::DashMap;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use utils::unified_log::{Direction, PaginatedLogs};
use uuid::Uuid;

/// Cache TTL in milliseconds. Short TTL ensures fresh data while reducing DB load.
const CACHE_TTL_MS: u64 = 200;

/// Maximum number of cache entries before eviction.
/// Each unique (assignment_id, cursor, limit, direction) tuple is one entry.
const MAX_CACHE_ENTRIES: usize = 1000;

/// Cache key for log pagination requests.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    assignment_id: Uuid,
    cursor: Option<i64>,
    limit: i64,
    direction: Direction,
}

impl CacheKey {
    fn new(assignment_id: Uuid, cursor: Option<i64>, limit: i64, direction: Direction) -> Self {
        Self {
            assignment_id,
            cursor,
            limit,
            direction,
        }
    }
}

/// Cached log entry with expiration time.
struct CacheEntry {
    data: PaginatedLogs,
    expires_at: Instant,
}

impl CacheEntry {
    fn new(data: PaginatedLogs) -> Self {
        Self {
            data,
            expires_at: Instant::now() + Duration::from_millis(CACHE_TTL_MS),
        }
    }

    fn is_expired(&self) -> bool {
        Instant::now() > self.expires_at
    }
}

/// Thread-safe in-memory cache for log pagination.
///
/// Uses DashMap for concurrent access without locks.
#[derive(Clone)]
pub struct LogCache {
    inner: Arc<DashMap<CacheKey, CacheEntry>>,
}

impl Default for LogCache {
    fn default() -> Self {
        Self::new()
    }
}

impl LogCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::with_capacity(256)),
        }
    }

    /// Get cached logs if available and not expired.
    pub fn get(
        &self,
        assignment_id: Uuid,
        cursor: Option<i64>,
        limit: i64,
        direction: Direction,
    ) -> Option<PaginatedLogs> {
        let key = CacheKey::new(assignment_id, cursor, limit, direction);

        // Use get() instead of entry() to avoid holding the lock
        let entry = self.inner.get(&key)?;

        if entry.is_expired() {
            // Entry is expired, remove it and return None
            drop(entry); // Release the read lock before removing
            self.inner.remove(&key);
            return None;
        }

        Some(entry.data.clone())
    }

    /// Cache a pagination result.
    pub fn set(
        &self,
        assignment_id: Uuid,
        cursor: Option<i64>,
        limit: i64,
        direction: Direction,
        data: PaginatedLogs,
    ) {
        // Evict if cache is too large
        if self.inner.len() >= MAX_CACHE_ENTRIES {
            self.evict_expired();

            // If still too large after evicting expired, remove oldest entries
            if self.inner.len() >= MAX_CACHE_ENTRIES {
                // Remove 10% of entries to make room
                let to_remove = MAX_CACHE_ENTRIES / 10;
                let keys_to_remove: Vec<_> = self
                    .inner
                    .iter()
                    .take(to_remove)
                    .map(|r| r.key().clone())
                    .collect();

                for key in keys_to_remove {
                    self.inner.remove(&key);
                }
            }
        }

        let key = CacheKey::new(assignment_id, cursor, limit, direction);
        self.inner.insert(key, CacheEntry::new(data));
    }

    /// Invalidate cache entries for a specific assignment.
    ///
    /// Call this when new logs are written for an assignment.
    pub fn invalidate_assignment(&self, assignment_id: Uuid) {
        self.inner
            .retain(|key, _| key.assignment_id != assignment_id);
    }

    /// Remove all expired entries.
    pub fn evict_expired(&self) {
        self.inner.retain(|_, entry| !entry.is_expired());
    }

    /// Clear all cache entries.
    pub fn clear(&self) {
        self.inner.clear();
    }

    /// Get the number of cached entries (for debugging/monitoring).
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Check if cache is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use utils::unified_log::LogEntry;

    fn make_paginated_logs(count: usize) -> PaginatedLogs {
        let entries: Vec<LogEntry> = (0..count)
            .map(|i| {
                LogEntry::new(
                    i as i64,
                    format!("log entry {}", i),
                    utils::unified_log::OutputType::Stdout,
                    chrono::Utc::now(),
                    Uuid::new_v4(),
                )
            })
            .collect();

        PaginatedLogs::new(entries, Some(count as i64), count > 0, Some(count as i64))
    }

    #[test]
    fn test_cache_set_and_get() {
        let cache = LogCache::new();
        let assignment_id = Uuid::new_v4();
        let data = make_paginated_logs(10);

        cache.set(assignment_id, None, 10, Direction::Backward, data.clone());

        let cached = cache.get(assignment_id, None, 10, Direction::Backward);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().entries.len(), 10);
    }

    #[test]
    fn test_cache_miss_different_params() {
        let cache = LogCache::new();
        let assignment_id = Uuid::new_v4();
        let data = make_paginated_logs(10);

        cache.set(assignment_id, None, 10, Direction::Backward, data);

        // Different limit
        assert!(cache.get(assignment_id, None, 20, Direction::Backward).is_none());

        // Different direction
        assert!(cache.get(assignment_id, None, 10, Direction::Forward).is_none());

        // Different cursor
        assert!(cache.get(assignment_id, Some(5), 10, Direction::Backward).is_none());

        // Different assignment
        assert!(cache.get(Uuid::new_v4(), None, 10, Direction::Backward).is_none());
    }

    #[test]
    fn test_cache_invalidation() {
        let cache = LogCache::new();
        let assignment_id = Uuid::new_v4();
        let other_assignment = Uuid::new_v4();

        cache.set(assignment_id, None, 10, Direction::Backward, make_paginated_logs(10));
        cache.set(assignment_id, Some(10), 10, Direction::Backward, make_paginated_logs(5));
        cache.set(other_assignment, None, 10, Direction::Backward, make_paginated_logs(3));

        assert_eq!(cache.len(), 3);

        // Invalidate one assignment
        cache.invalidate_assignment(assignment_id);

        assert_eq!(cache.len(), 1);
        assert!(cache.get(assignment_id, None, 10, Direction::Backward).is_none());
        assert!(cache.get(other_assignment, None, 10, Direction::Backward).is_some());
    }

    #[test]
    fn test_cache_clear() {
        let cache = LogCache::new();
        let assignment_id = Uuid::new_v4();

        cache.set(assignment_id, None, 10, Direction::Backward, make_paginated_logs(10));
        cache.set(assignment_id, Some(10), 10, Direction::Backward, make_paginated_logs(5));

        assert_eq!(cache.len(), 2);

        cache.clear();

        assert!(cache.is_empty());
    }

    #[tokio::test]
    async fn test_cache_expiration() {
        let cache = LogCache::new();
        let assignment_id = Uuid::new_v4();
        let data = make_paginated_logs(10);

        cache.set(assignment_id, None, 10, Direction::Backward, data);

        // Should be available immediately
        assert!(cache.get(assignment_id, None, 10, Direction::Backward).is_some());

        // Wait for TTL to expire (200ms + buffer)
        tokio::time::sleep(Duration::from_millis(250)).await;

        // Should be expired now
        assert!(cache.get(assignment_id, None, 10, Direction::Backward).is_none());
    }
}
