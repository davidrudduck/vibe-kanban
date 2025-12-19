/**
 * useUnifiedLogs - Unified log access hook with pagination and live streaming
 *
 * This hook provides a unified interface for accessing logs from both local and remote
 * execution processes. It uses:
 * - REST API (GET /api/logs/{execution_id}) for paginated historical data
 * - WebSocket (WS /api/logs/{execution_id}/live) for live streaming updates
 *
 * Usage:
 * ```tsx
 * const { entries, isLoading, hasMore, loadMore, isLive } = useUnifiedLogs({
 *   executionId: '...',
 *   initialLimit: 100,
 * });
 * ```
 */
import { useState, useEffect, useRef, useCallback } from 'react';
import { logsApi } from '@/lib/api';
import type { LogEntry, OutputType, PaginatedLogs } from 'shared/types';

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

  // Initial load of historical entries
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

        // Load initial entries (newest first, using backward direction)
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
        setIsLoading(false);
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
  }, [executionId, initialLimit]);

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
      };

      ws.onmessage = (event) => {
        if (!mountedRef.current) return;

        try {
          const data = JSON.parse(event.data);

          // Handle LogMsg types from the backend
          if ('JsonPatch' in data) {
            // JsonPatch contains an array of patches with log content
            const patches = data.JsonPatch as Array<{ value?: { type: string; content: string } }>;
            const newEntries: LogEntry[] = [];

            patches.forEach((patch) => {
              const value = patch?.value;
              if (!value || !value.type) return;

              // Map LogMsg types to LogEntry
              if (value.type === 'STDOUT' || value.type === 'STDERR') {
                const outputType: OutputType = value.type === 'STDOUT' ? 'stdout' : 'stderr';
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
  }, [executionId, enableLiveStream, isLoading, connectionToken]);

  // Load more historical entries (for scroll-to-top pagination)
  const loadMore = useCallback(async () => {
    if (!executionId || !hasMore || isLoadingMoreRef.current || nextCursorRef.current === null) {
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
    } catch (err) {
      if (!mountedRef.current) return;
      console.error('Failed to load more logs:', err);
      setError(err instanceof Error ? err.message : 'Failed to load more logs');
    } finally {
      isLoadingMoreRef.current = false;
    }
  }, [executionId, hasMore]);

  return {
    entries,
    isLoading,
    hasMore,
    loadMore,
    isLive,
    error,
    totalCount,
  };
}

export default useUnifiedLogs;
