/**
 * useUnifiedLogs - Unified log access hook with pagination, live streaming, and caching
 *
 * This hook provides a unified interface for accessing logs from both local and remote
 * execution processes. It uses:
 * - REST API (GET /api/logs/{execution_id}) for paginated historical data
 * - WebSocket (WS /api/logs/{execution_id}/live) for live streaming updates
 * - Client-side cache with TTL and LRU eviction for fast navigation
 *
 * Cache behavior:
 * - Cached data is used for instant load when navigating back to a conversation
 * - Cache expires after 5 minutes (TTL)
 * - Maximum 10 conversations cached (LRU eviction)
 * - Running processes skip caching (data changes too frequently)
 *
 * Usage:
 * ```tsx
 * // Basic usage with manual limit
 * const { entries, isLoading, hasMore, loadMore, isLive, isCached } = useUnifiedLogs({
 *   executionId: '...',
 *   initialLimit: 100,
 * });
 *
 * // With config-aware pagination (recommended)
 * const { entries, isLoading, hasMore, loadMore, effectiveLimit, setOverride } = useUnifiedLogsWithConfig({
 *   executionId: '...',
 * });
 * ```
 */
import { useState, useEffect, useRef, useCallback } from 'react';
import { logsApi } from '@/lib/api';
import type { LogEntry, OutputType, PaginatedLogs } from 'shared/types';
import { useLogCacheStore } from '@/stores/useLogCache';
import { useEffectivePagination } from './useEffectivePagination';
import type { PaginationPreset } from '@/stores/usePaginationOverride';

export interface UseUnifiedLogsOptions {
  /** The execution process ID to fetch logs for */
  executionId: string;
  /** Initial number of entries to load (default: 100) */
  initialLimit?: number;
  /** Whether to enable live streaming for running processes (default: true) */
  enableLiveStream?: boolean;
  /** Optional connection token for external/remote access */
  connectionToken?: string;
}

export interface UseUnifiedLogsResult {
  /** The log entries (historical + live) */
  entries: LogEntry[];
  /** Whether the initial load is in progress */
  isLoading: boolean;
  /** Whether there are more historical entries to load */
  hasMore: boolean;
  /** Load more historical entries (for scroll-triggered pagination) */
  loadMore: () => Promise<void>;
  /** Whether live streaming is currently active */
  isLive: boolean;
  /** Error message if any */
  error: string | null;
  /** Total count of entries (if available from server) */
  totalCount: bigint | null;
  /** Whether data was loaded from cache (instant load) */
  isCached: boolean;
}

const DEFAULT_INITIAL_LIMIT = 100;
const LOAD_MORE_LIMIT = 50;

export function useUnifiedLogs({
  executionId,
  initialLimit = DEFAULT_INITIAL_LIMIT,
  enableLiveStream = true,
  connectionToken,
}: UseUnifiedLogsOptions): UseUnifiedLogsResult {
  const [entries, setEntries] = useState<LogEntry[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [hasMore, setHasMore] = useState(false);
  const [isLive, setIsLive] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [totalCount, setTotalCount] = useState<bigint | null>(null);
  const [isCached, setIsCached] = useState(false);

  // Cache store methods
  const getCached = useLogCacheStore((s) => s.getCached);
  const setCached = useLogCacheStore((s) => s.setCached);
  const appendOlderEntries = useLogCacheStore((s) => s.appendOlderEntries);
  const invalidateCache = useLogCacheStore((s) => s.invalidate);

  // Refs for tracking state across renders
  const nextCursorRef = useRef<bigint | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const isLoadingMoreRef = useRef(false);
  const mountedRef = useRef(true);
  const retryCountRef = useRef(0);
  const retryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Reset state when execution ID changes
  useEffect(() => {
    setEntries([]);
    setIsLoading(true);
    setHasMore(false);
    setIsLive(false);
    setError(null);
    setTotalCount(null);
    setIsCached(false);
    nextCursorRef.current = null;
    isLoadingMoreRef.current = false;
    retryCountRef.current = 0;

    // Clean up existing WebSocket
    if (wsRef.current) {
      wsRef.current.close();
      wsRef.current = null;
    }
    if (retryTimerRef.current) {
      clearTimeout(retryTimerRef.current);
      retryTimerRef.current = null;
    }
  }, [executionId]);

  // Initial load of historical entries (with cache check)
  useEffect(() => {
    if (!executionId) {
      setIsLoading(false);
      return;
    }

    mountedRef.current = true;

    const loadInitial = async () => {
      try {
        setIsLoading(true);
        setError(null);

        // Check cache first for instant load
        const cached = getCached(executionId);
        if (cached) {
          // Use cached data
          setEntries(cached.entries);
          setHasMore(cached.hasMore);
          nextCursorRef.current = cached.nextCursor;
          if (cached.totalCount !== null) {
            setTotalCount(cached.totalCount);
          }
          setIsCached(true);
          setIsLoading(false);
          return;
        }

        // No cache hit, fetch from server
        const result: PaginatedLogs = await logsApi.getPaginated(executionId, {
          limit: initialLimit,
          direction: 'backward',
        });

        if (!mountedRef.current) return;

        // Reverse entries to display oldest first (for chat-like view)
        const reversedEntries = [...result.entries].reverse();
        setEntries(reversedEntries);
        setHasMore(result.has_more);
        nextCursorRef.current = result.next_cursor;
        if (result.total_count !== null) {
          setTotalCount(result.total_count);
        }
        setIsCached(false);
        setIsLoading(false);

        // Cache the result (will be skipped if process is running, detected via WebSocket)
        // Note: We'll update cache status based on isLive state after WebSocket connects
        setCached(
          executionId,
          {
            entries: reversedEntries,
            nextCursor: result.next_cursor,
            hasMore: result.has_more,
            totalCount: result.total_count,
          },
          false // Not running initially; WebSocket will determine live status
        );
      } catch (err) {
        if (!mountedRef.current) return;
        console.error('Failed to load initial logs:', err);
        setError(err instanceof Error ? err.message : 'Failed to load logs');
        setIsLoading(false);
      }
    };

    loadInitial();

    return () => {
      mountedRef.current = false;
    };
  }, [executionId, initialLimit, getCached, setCached]);

  // Set up WebSocket for live streaming
  useEffect(() => {
    if (!executionId || !enableLiveStream || isLoading) {
      return;
    }

    const connectWebSocket = () => {
      const wsUrl = logsApi.getLiveStreamUrl(executionId, connectionToken);
      const ws = new WebSocket(wsUrl);
      wsRef.current = ws;

      ws.onopen = () => {
        if (!mountedRef.current) return;
        setIsLive(true);
        setError(null);
        retryCountRef.current = 0;
        // Invalidate cache for running processes - data is actively changing
        invalidateCache(executionId);
        setIsCached(false);
      };

      ws.onmessage = (event) => {
        if (!mountedRef.current) return;

        try {
          const data = JSON.parse(event.data);

          // Handle LogMsg types from the backend
          if ('JsonPatch' in data) {
            // JsonPatch contains an array of patches with log content
            const patches = data.JsonPatch as Array<{
              value?: { type: string; content: string };
            }>;
            const newEntries: LogEntry[] = [];

            patches.forEach((patch) => {
              const value = patch?.value;
              if (!value || !value.type) return;

              // Map LogMsg types to LogEntry
              if (value.type === 'STDOUT' || value.type === 'STDERR') {
                const outputType: OutputType =
                  value.type === 'STDOUT' ? 'stdout' : 'stderr';
                const entry: LogEntry = {
                  id: BigInt(Date.now()), // Use timestamp as temporary ID for live entries
                  content: value.content,
                  output_type: outputType,
                  timestamp: new Date().toISOString(),
                  execution_id: executionId,
                };
                newEntries.push(entry);
              }
            });

            if (newEntries.length > 0) {
              setEntries((prev) => [...prev, ...newEntries]);
            }
          } else if ('Stdout' in data) {
            const entry: LogEntry = {
              id: BigInt(Date.now()),
              content: data.Stdout,
              output_type: 'stdout' as OutputType,
              timestamp: new Date().toISOString(),
              execution_id: executionId,
            };
            setEntries((prev) => [...prev, entry]);
          } else if ('Stderr' in data) {
            const entry: LogEntry = {
              id: BigInt(Date.now()),
              content: data.Stderr,
              output_type: 'stderr' as OutputType,
              timestamp: new Date().toISOString(),
              execution_id: executionId,
            };
            setEntries((prev) => [...prev, entry]);
          } else if ('Finished' in data || data.finished === true) {
            // Process has finished, close WebSocket
            setIsLive(false);
            ws.close();
          }
        } catch (err) {
          console.error('Failed to parse WebSocket message:', err);
        }
      };

      ws.onerror = () => {
        if (!mountedRef.current) return;
        // Don't set error for expected 404s (process not running)
        // The live stream is optional, REST pagination still works
      };

      ws.onclose = (event) => {
        if (!mountedRef.current) return;
        setIsLive(false);

        // Only retry for unexpected closures (not 1000 normal close, not 1006 going away)
        if (event.code !== 1000 && event.code !== 1006) {
          const next = retryCountRef.current + 1;
          retryCountRef.current = next;

          if (next <= 3) {
            const delay = Math.min(3000, 500 * 2 ** (next - 1));
            retryTimerRef.current = setTimeout(() => {
              if (mountedRef.current) {
                connectWebSocket();
              }
            }, delay);
          }
        }
      };
    };

    connectWebSocket();

    return () => {
      if (wsRef.current) {
        wsRef.current.close();
        wsRef.current = null;
      }
      if (retryTimerRef.current) {
        clearTimeout(retryTimerRef.current);
        retryTimerRef.current = null;
      }
    };
  }, [
    executionId,
    enableLiveStream,
    isLoading,
    connectionToken,
    invalidateCache,
  ]);

  // Load more historical entries (for scroll-to-top pagination)
  const loadMore = useCallback(async () => {
    if (
      !executionId ||
      !hasMore ||
      isLoadingMoreRef.current ||
      nextCursorRef.current === null
    ) {
      return;
    }

    isLoadingMoreRef.current = true;

    try {
      const result: PaginatedLogs = await logsApi.getPaginated(executionId, {
        limit: LOAD_MORE_LIMIT,
        cursor: nextCursorRef.current,
        direction: 'backward',
      });

      if (!mountedRef.current) return;

      // Prepend older entries (reversed to maintain chronological order)
      const olderEntries = [...result.entries].reverse();
      setEntries((prev) => [...olderEntries, ...prev]);
      setHasMore(result.has_more);
      nextCursorRef.current = result.next_cursor;

      // Update cache with the additional older entries (only if not live streaming)
      if (!isLive) {
        appendOlderEntries(
          executionId,
          olderEntries,
          result.next_cursor,
          result.has_more
        );
      }
    } catch (err) {
      if (!mountedRef.current) return;
      console.error('Failed to load more logs:', err);
      setError(err instanceof Error ? err.message : 'Failed to load more logs');
    } finally {
      isLoadingMoreRef.current = false;
    }
  }, [executionId, hasMore, isLive, appendOlderEntries]);

  return {
    entries,
    isLoading,
    hasMore,
    loadMore,
    isLive,
    error,
    totalCount,
    isCached,
  };
}

/**
 * Options for useUnifiedLogsWithConfig - config-aware version
 */
export interface UseUnifiedLogsWithConfigOptions {
  /** The execution process ID to fetch logs for */
  executionId: string;
  /** Whether to enable live streaming for running processes (default: true) */
  enableLiveStream?: boolean;
  /** Optional connection token for external/remote access */
  connectionToken?: string;
}

/**
 * Result from useUnifiedLogsWithConfig - includes pagination control
 */
export interface UseUnifiedLogsWithConfigResult extends UseUnifiedLogsResult {
  /** The effective pagination limit being used */
  effectiveLimit: number;
  /** The global pagination limit from config */
  globalLimit: number;
  /** Current override value ('global' means using global setting) */
  override: PaginationPreset;
  /** Set a per-conversation override */
  setOverride: (value: PaginationPreset) => void;
  /** Whether an override is active */
  hasOverride: boolean;
}

/**
 * Config-aware version of useUnifiedLogs
 *
 * Automatically uses the global pagination setting from config,
 * with support for per-conversation overrides.
 *
 * @example
 * ```tsx
 * const {
 *   entries,
 *   isLoading,
 *   effectiveLimit,
 *   setOverride,
 *   hasOverride
 * } = useUnifiedLogsWithConfig({ executionId });
 *
 * // Change pagination for this conversation only
 * setOverride(200);
 *
 * // Reset to global setting
 * setOverride('global');
 * ```
 */
export function useUnifiedLogsWithConfig({
  executionId,
  enableLiveStream = true,
  connectionToken,
}: UseUnifiedLogsWithConfigOptions): UseUnifiedLogsWithConfigResult {
  const { effectiveLimit, globalLimit, override, setOverride, hasOverride } =
    useEffectivePagination(executionId);

  const result = useUnifiedLogs({
    executionId,
    initialLimit: effectiveLimit,
    enableLiveStream,
    connectionToken,
  });

  return {
    ...result,
    effectiveLimit,
    globalLimit,
    override,
    setOverride,
    hasOverride,
  };
}

export default useUnifiedLogs;
