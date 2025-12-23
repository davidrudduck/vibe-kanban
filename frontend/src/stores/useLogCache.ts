import { create } from 'zustand';
import type { LogEntry } from 'shared/types';

/**
 * Client-side log cache with LRU eviction and TTL expiration.
 *
 * Features:
 * - TTL: 5 minutes (entries expire after this time)
 * - Max entries: 10 conversations (LRU eviction when exceeded)
 * - Skip caching for running processes (live WebSocket data)
 *
 * Usage:
 * ```tsx
 * const { getCached, setCached, invalidate } = useLogCacheStore();
 *
 * // Check cache before fetching
 * const cached = getCached(executionId);
 * if (cached) {
 *   // Use cached data
 * } else {
 *   // Fetch from server
 *   setCached(executionId, data, isRunning);
 * }
 * ```
 */

const CACHE_TTL_MS = 5 * 60 * 1000; // 5 minutes
const MAX_CACHE_SIZE = 10; // Max conversations to cache

export interface CachedLogData {
  /** The log entries */
  entries: LogEntry[];
  /** Cursor for fetching more entries */
  nextCursor: bigint | null;
  /** Whether there are more entries to fetch */
  hasMore: boolean;
  /** Total count if available */
  totalCount: bigint | null;
  /** Timestamp when this was cached */
  cachedAt: number;
  /** Last access timestamp (for LRU) */
  lastAccessedAt: number;
}

interface LogCacheState {
  /**
   * Map of execution_id -> cached log data
   */
  cache: Record<string, CachedLogData>;

  /**
   * Get cached data for an execution if it exists and is not expired.
   * Updates lastAccessedAt for LRU tracking.
   * Returns null if not cached, expired, or stale.
   */
  getCached: (executionId: string) => CachedLogData | null;

  /**
   * Set cached data for an execution.
   * Skips caching if the process is currently running.
   * Performs LRU eviction if cache is full.
   *
   * @param executionId - The execution ID
   * @param data - Partial data to cache (entries, nextCursor, hasMore, totalCount)
   * @param isRunning - Whether the process is currently running (skip cache if true)
   */
  setCached: (
    executionId: string,
    data: Pick<CachedLogData, 'entries' | 'nextCursor' | 'hasMore' | 'totalCount'>,
    isRunning?: boolean
  ) => void;

  /**
   * Update cached entries with additional entries (for pagination).
   * Prepends new entries to existing cached entries.
   */
  appendOlderEntries: (
    executionId: string,
    olderEntries: LogEntry[],
    nextCursor: bigint | null,
    hasMore: boolean
  ) => void;

  /**
   * Invalidate (remove) cached data for a specific execution.
   */
  invalidate: (executionId: string) => void;

  /**
   * Clear all cached data.
   */
  clearAll: () => void;

  /**
   * Get cache statistics for debugging.
   */
  getStats: () => { size: number; entries: Array<{ id: string; age: number; hits: number }> };
}

/**
 * Perform LRU eviction to maintain max cache size.
 * Removes the least recently accessed entry.
 */
function evictLRU(cache: Record<string, CachedLogData>): Record<string, CachedLogData> {
  const entries = Object.entries(cache);
  if (entries.length <= MAX_CACHE_SIZE) {
    return cache;
  }

  // Sort by lastAccessedAt ascending (oldest first)
  entries.sort((a, b) => a[1].lastAccessedAt - b[1].lastAccessedAt);

  // Remove the oldest entry
  const [oldestId] = entries[0];
  const { [oldestId]: _removed, ...rest } = cache;
  void _removed; // Intentionally unused
  return rest;
}

/**
 * Check if a cache entry is expired.
 */
function isExpired(cachedAt: number): boolean {
  return Date.now() - cachedAt > CACHE_TTL_MS;
}

export const useLogCacheStore = create<LogCacheState>((set, get) => ({
  cache: {},

  getCached: (executionId) => {
    const { cache } = get();
    const cached = cache[executionId];

    if (!cached) {
      return null;
    }

    // Check TTL expiration
    if (isExpired(cached.cachedAt)) {
      // Remove expired entry
      set((state) => {
        const { [executionId]: _expired, ...rest } = state.cache;
        void _expired; // Intentionally unused
        return { cache: rest };
      });
      return null;
    }

    // Update lastAccessedAt for LRU tracking
    set((state) => ({
      cache: {
        ...state.cache,
        [executionId]: {
          ...cached,
          lastAccessedAt: Date.now(),
        },
      },
    }));

    return cached;
  },

  setCached: (executionId, data, isRunning = false) => {
    // Skip caching for running processes - their data is constantly changing
    if (isRunning) {
      return;
    }

    const now = Date.now();
    const newEntry: CachedLogData = {
      entries: data.entries,
      nextCursor: data.nextCursor,
      hasMore: data.hasMore,
      totalCount: data.totalCount,
      cachedAt: now,
      lastAccessedAt: now,
    };

    set((state) => {
      // First, clean up any expired entries
      const cleanedCache: Record<string, CachedLogData> = {};
      for (const [id, entry] of Object.entries(state.cache)) {
        if (!isExpired(entry.cachedAt)) {
          cleanedCache[id] = entry;
        }
      }

      // Add new entry
      cleanedCache[executionId] = newEntry;

      // Perform LRU eviction if needed
      const finalCache = evictLRU(cleanedCache);

      return { cache: finalCache };
    });
  },

  appendOlderEntries: (executionId, olderEntries, nextCursor, hasMore) => {
    set((state) => {
      const existing = state.cache[executionId];
      if (!existing || isExpired(existing.cachedAt)) {
        // Don't update if not cached or expired
        return state;
      }

      return {
        cache: {
          ...state.cache,
          [executionId]: {
            ...existing,
            entries: [...olderEntries, ...existing.entries],
            nextCursor,
            hasMore,
            lastAccessedAt: Date.now(),
          },
        },
      };
    });
  },

  invalidate: (executionId) => {
    set((state) => {
      const { [executionId]: _invalidated, ...rest } = state.cache;
      void _invalidated; // Intentionally unused
      return { cache: rest };
    });
  },

  clearAll: () => {
    set({ cache: {} });
  },

  getStats: () => {
    const { cache } = get();
    const now = Date.now();
    const entries = Object.entries(cache).map(([id, data]) => ({
      id,
      age: Math.round((now - data.cachedAt) / 1000), // age in seconds
      hits: 0, // We don't track hits currently, but could add
    }));
    return {
      size: entries.length,
      entries,
    };
  },
}));

/**
 * Hook to check if an execution has cached data.
 * Useful for UI indicators.
 */
export function useHasCachedLogs(executionId: string): boolean {
  return useLogCacheStore((state) => {
    const cached = state.cache[executionId];
    return cached !== undefined && !isExpired(cached.cachedAt);
  });
}

/**
 * Export constants for testing/debugging.
 */
export const CACHE_CONFIG = {
  TTL_MS: CACHE_TTL_MS,
  MAX_SIZE: MAX_CACHE_SIZE,
};
