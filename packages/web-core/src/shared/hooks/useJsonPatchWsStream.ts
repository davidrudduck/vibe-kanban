import { useEffect, useState, useRef } from 'react';
import { produce } from 'immer';
import type { Operation } from 'rfc6902';
import { applyUpsertPatch } from '@/shared/lib/jsonPatch';
import { openLocalApiWebSocket } from '@/shared/lib/localApiTransport';
import { useDocumentVisible } from '@/shared/hooks/useDocumentVisible';

type WsJsonPatchMsg = { JsonPatch: Operation[] };
type WsReadyMsg = { Ready: true };
type WsFinishedMsg = { finished: boolean };
type WsMsg = WsJsonPatchMsg | WsReadyMsg | WsFinishedMsg;

interface UseJsonPatchStreamOptions<T> {
  /**
   * Called once when the stream starts to inject initial data
   */
  injectInitialEntry?: (data: T) => void;
  /**
   * Filter/deduplicate patches before applying them
   */
  deduplicatePatches?: (patches: Operation[]) => Operation[];
}

interface UseJsonPatchStreamResult<T> {
  data: T | undefined;
  isConnected: boolean;
  isInitialized: boolean;
  /**
   * True when the socket is connected but we are still waiting for the
   * initial snapshot (Ready message) to be applied.
   */
  isSyncing: boolean;
  error: string | null;
}

/**
 * Generic hook for consuming WebSocket streams that send JSON messages with patches
 */
export const useJsonPatchWsStream = <T extends object>(
  endpoint: string | undefined,
  enabled: boolean,
  initialData: () => T,
  options?: UseJsonPatchStreamOptions<T>
): UseJsonPatchStreamResult<T> => {
  const [data, setData] = useState<T | undefined>(undefined);
  const [isConnected, setIsConnected] = useState(false);
  const [isSyncing, setIsSyncing] = useState(false);
  const [isInitialized, setIsInitialized] = useState(false);
  const initializedForEndpointRef = useRef<string | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const dataRef = useRef<T | undefined>(undefined);
  const activeEndpointRef = useRef<string | undefined>(undefined);
  const retryTimerRef = useRef<number | null>(null);
  const retryAttemptsRef = useRef<number>(0);
  const [retryNonce, setRetryNonce] = useState(0);
  const finishedRef = useRef<boolean>(false);

  // Idle-timeout watchdog: detects silently dead WebSocket connections that
  // never trigger `onclose` (e.g. a half-open TCP connection after sleep/wake,
  // a flaky proxy, or a backend that stopped emitting because of an upstream
  // hang). If no message arrives for IDLE_TIMEOUT_MS we force-close the
  // socket; the existing reconnect logic in `onclose` then re-handshakes
  // and replays the snapshot, restoring fresh state without a page refresh.
  const lastActivityRef = useRef<number>(Date.now());
  const watchdogIntervalRef = useRef<number | null>(null);
  // Reasonable defaults: backend pushes a `Ready` shortly after open and
  // patches whenever state changes. 90s of total silence is well above any
  // legitimate quiet window for an active workspace stream.
  const WATCHDOG_CHECK_MS = 5000;
  const IDLE_TIMEOUT_MS = 90000;
  // Maximum time to wait for the initial snapshot (Ready message) before
  // assuming the connection is stuck and forcing a reconnect.
  const INITIAL_SYNC_TIMEOUT_MS = 30000;

  const injectInitialEntry = options?.injectInitialEntry;
  const deduplicatePatches = options?.deduplicatePatches;

  const documentVisible = useDocumentVisible();

  function scheduleReconnect() {
    if (retryTimerRef.current) return; // already scheduled
    // Exponential backoff with cap: 1s, 2s, 4s, 8s (max), then stay at 8s
    const attempt = retryAttemptsRef.current;
    const delay = Math.min(8000, 1000 * Math.pow(2, attempt));
    retryTimerRef.current = window.setTimeout(() => {
      retryTimerRef.current = null;
      setRetryNonce((n) => n + 1);
    }, delay);
  }

  useEffect(() => {
    // Case 1: intentionally disabled or no endpoint — full teardown including state
    if (!enabled || !endpoint) {
      if (wsRef.current) {
        wsRef.current.close();
        wsRef.current = null;
      }
      if (retryTimerRef.current) {
        window.clearTimeout(retryTimerRef.current);
        retryTimerRef.current = null;
      }
      if (watchdogIntervalRef.current) {
        window.clearInterval(watchdogIntervalRef.current);
        watchdogIntervalRef.current = null;
      }
      retryAttemptsRef.current = 0;
      finishedRef.current = false;
      setData(undefined);
      setIsConnected(false);
      setIsSyncing(false);
      setIsInitialized(false);
      setError(null);
      dataRef.current = undefined;
      activeEndpointRef.current = undefined;
      initializedForEndpointRef.current = undefined;
      return;
    }

    // Case 2: tab hidden — close WS but preserve React state so the UI stays
    // populated. Reset dataRef so the server snapshot applies to a clean slate
    // on reconnect (avoiding stale-delete gaps), without flashing a blank screen.
    // Exception: if the base path changed while hidden (e.g. programmatic navigation
    // to a different workspace), clear React state immediately so stale data from
    // the old stream isn't shown when the tab returns.
    if (!documentVisible) {
      if (wsRef.current) {
        wsRef.current.close();
        wsRef.current = null;
      }
      if (retryTimerRef.current) {
        window.clearTimeout(retryTimerRef.current);
        retryTimerRef.current = null;
      }
      if (watchdogIntervalRef.current) {
        window.clearInterval(watchdogIntervalRef.current);
        watchdogIntervalRef.current = null;
      }
      retryAttemptsRef.current = 0;
      finishedRef.current = false;
      setIsConnected(false);
      setIsSyncing(false);
      const prevBase = activeEndpointRef.current?.split('?')[0];
      const newBase = endpoint.split('?')[0];
      if (prevBase !== undefined && prevBase !== newBase) {
        setData(undefined);
        setIsInitialized(false);
      }
      // Always reset so the error-suppression guard (!initializedForEndpointRef.current)
      // works correctly if reconnect fails after the tab returns.
      initializedForEndpointRef.current = undefined;
      dataRef.current = undefined;
      activeEndpointRef.current = endpoint;
      return;
    }

    if (activeEndpointRef.current !== endpoint) {
      const prevBase = activeEndpointRef.current?.split('?')[0];
      const newBase = endpoint.split('?')[0];

      activeEndpointRef.current = endpoint;
      initializedForEndpointRef.current = undefined;

      if (prevBase !== newBase) {
        // Different stream (e.g. navigated to a different workspace) — full reset
        dataRef.current = undefined;
        setData(undefined);
        setIsInitialized(false);
      } else {
        // Same stream, different query params (e.g. stats_only toggle) — keep
        // React state so UI stays populated, but reset dataRef so the server's
        // fresh snapshot applies to a clean slate on reconnect.
        dataRef.current = undefined;
      }
      retryAttemptsRef.current = 0;
    }

    let cancelled = false;

    // Create WebSocket if it doesn't exist
    if (!wsRef.current) {
      // Reset finished flag for new connection
      finishedRef.current = false;

      void (async () => {
        try {
          // openLocalApiWebSocket defers new WebSocket() by a microtask, ensuring
          // StrictMode cleanup can cancel before the TCP upgrade is initiated.
          const ws = await openLocalApiWebSocket(endpoint);

          if (cancelled) {
            ws.close();
            return;
          }

          // Reset dataRef so the server's fresh snapshot applies to a clean slate
          // on this connection, while preserving the old React state (data) on
          // screen until the new Ready message arrives.
          dataRef.current = initialData();
          if (injectInitialEntry) {
            injectInitialEntry(dataRef.current);
          }

          // Patches received before the Ready message are buffered here and
          // flushed atomically when Ready arrives. This prevents the UI from
          // flickering through partial states when the server sends the initial
          // snapshot across multiple messages. Once Ready fires, snapshotBuffer
          // is set to null and subsequent patches are applied live.
          let snapshotBuffer: Operation[] | null = [];
          const connectionOpenTime = Date.now();
          ws.onopen = () => {
            setError(null);
            setIsConnected(true);
            setIsSyncing(true);
            if (retryTimerRef.current) {
              window.clearTimeout(retryTimerRef.current);
              retryTimerRef.current = null;
            }

            // Start the idle watchdog. We treat the open event itself as
            // activity so the timer doesn't fire before the first message.
            lastActivityRef.current = Date.now();
            if (watchdogIntervalRef.current) {
              window.clearInterval(watchdogIntervalRef.current);
            }
            watchdogIntervalRef.current = window.setInterval(() => {
              const now = Date.now();
              const idleMs = now - lastActivityRef.current;
              const syncDurationMs = now - connectionOpenTime;

              if (wsRef.current !== ws) return;

              // Force reconnect if:
              // 1. Total silence for IDLE_TIMEOUT_MS
              // 2. We are still waiting for the initial snapshot (Ready) after INITIAL_SYNC_TIMEOUT_MS
              if (
                idleMs > IDLE_TIMEOUT_MS ||
                (snapshotBuffer !== null &&
                  syncDurationMs > INITIAL_SYNC_TIMEOUT_MS)
              ) {
                console.warn(
                  `[useJsonPatchWsStream] ${
                    snapshotBuffer !== null ? 'sync' : 'idle'
                  } timeout reached, forcing reconnect`
                );
                // Non-1000 close code so the reconnect logic in onclose runs.
                try {
                  ws.close(4000, 'timeout');
                } catch {
                  // ignore
                }
              }
            }, WATCHDOG_CHECK_MS);
          };

          ws.onmessage = (event) => {
            // Any inbound message counts as activity for the watchdog.
            lastActivityRef.current = Date.now();
            try {
              const msg: WsMsg = JSON.parse(event.data);

              // Handle JsonPatch messages (same as SSE json_patch event)
              if ('JsonPatch' in msg) {
                const patches: Operation[] = msg.JsonPatch;
                const filtered = deduplicatePatches
                  ? deduplicatePatches(patches)
                  : patches;

                if (!filtered.length) return;

                if (snapshotBuffer !== null) {
                  // Pre-Ready: accumulate patches, don't touch React state yet
                  snapshotBuffer.push(...filtered);
                  return;
                }

                const current = dataRef.current;
                if (!current) return;

                // Use Immer for structural sharing - only modified parts get new references
                const next = produce(current, (draft) => {
                  applyUpsertPatch(draft, filtered);
                });

                dataRef.current = next;
                setData(next);
              }

              // Handle Ready messages (initial data has been sent)
              if ('Ready' in msg) {
                // Flush buffered snapshot patches atomically so the UI updates
                // in one render rather than flickering through partial states.
                // Note: we always flush here even if the buffer is empty to ensure
                // that React state is synced with dataRef.current (which was reset
                // to initialData on reconnect).
                if (snapshotBuffer !== null) {
                  const current = dataRef.current;
                  if (current) {
                    const next = produce(current, (draft) => {
                      applyUpsertPatch(draft, snapshotBuffer!);
                    });
                    dataRef.current = next;
                    setData(next);
                  }
                }
                snapshotBuffer = null;
                // Reset backoff only after a confirmed snapshot — prevents
                // the open→close-before-Ready loop from resetting the counter
                // on every onopen and suppressing the error display.
                retryAttemptsRef.current = 0;
                initializedForEndpointRef.current = endpoint;
                setIsInitialized(true);
                setIsSyncing(false);
                setError(null);
              }

              // Handle finished messages ({finished: true})
              // Treat finished as terminal - do NOT reconnect
              if ('finished' in msg) {
                finishedRef.current = true;
                ws.close(1000, 'finished');
                wsRef.current = null;
                setIsConnected(false);
                setIsSyncing(false);
              }
            } catch (err) {
              console.error('Failed to process WebSocket message:', err);
              setError('Failed to process stream update');
            }
          };

          ws.onerror = () => {
            // Don't set error here — onclose always fires after onerror
            // and handles retry logic. Setting error eagerly hides data
            // that was already received.
          };

          ws.onclose = (evt) => {
            setIsConnected(false);
            setIsSyncing(false);
            wsRef.current = null;

            // Stop the idle watchdog for this connection; the next onopen
            // (after reconnect) will start a fresh one.
            if (watchdogIntervalRef.current) {
              window.clearInterval(watchdogIntervalRef.current);
              watchdogIntervalRef.current = null;
            }

            // Do not reconnect if we received a finished message or clean close
            if (
              cancelled ||
              finishedRef.current ||
              (evt?.code === 1000 && evt?.wasClean)
            ) {
              return;
            }

            // Otherwise, reconnect on unexpected/error closures
            retryAttemptsRef.current += 1;
            // Only show error if the server never sent a Ready for this endpoint
            if (
              !initializedForEndpointRef.current &&
              retryAttemptsRef.current > 6
            ) {
              setError('Connection failed');
            }
            scheduleReconnect();
          };

          wsRef.current = ws;
        } catch (error) {
          if (cancelled) {
            return;
          }

          console.error('Failed to open WebSocket stream:', error);
          setIsSyncing(false);
          retryAttemptsRef.current += 1;
          if (
            !initializedForEndpointRef.current &&
            retryAttemptsRef.current > 6
          ) {
            setError('Connection failed');
          }
          scheduleReconnect();
        }
      })();
    }

    return () => {
      cancelled = true;
      if (wsRef.current) {
        const ws = wsRef.current;

        // Clear all event handlers first to prevent callbacks after cleanup
        ws.onopen = null;
        ws.onmessage = null;
        ws.onerror = null;
        ws.onclose = null;

        // Close regardless of state
        ws.close();
        wsRef.current = null;
      }
      if (retryTimerRef.current) {
        window.clearTimeout(retryTimerRef.current);
        retryTimerRef.current = null;
      }
      if (watchdogIntervalRef.current) {
        window.clearInterval(watchdogIntervalRef.current);
        watchdogIntervalRef.current = null;
      }
      finishedRef.current = false;
    };
  }, [
    endpoint,
    enabled,
    documentVisible,
    initialData,
    injectInitialEntry,
    deduplicatePatches,
    retryNonce,
  ]);

  const isInitializedForCurrentEndpoint =
    isInitialized && initializedForEndpointRef.current === endpoint;

  return {
    data,
    isConnected,
    isInitialized: isInitializedForCurrentEndpoint,
    isSyncing,
    error,
  };
};
