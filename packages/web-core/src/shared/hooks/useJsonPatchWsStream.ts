import { useEffect, useState, useRef } from 'react';
import { produce } from 'immer';
import type { Operation } from 'rfc6902';
import { applyUpsertPatch } from '@/shared/lib/jsonPatch';
import { openLocalApiWebSocket } from '@/shared/lib/localApiTransport';

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
  const [isInitialized, setIsInitialized] = useState(false);
  const initializedForEndpointRef = useRef<string | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const dataRef = useRef<T | undefined>(undefined);
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
  const WATCHDOG_CHECK_MS = 15000;
  const IDLE_TIMEOUT_MS = 90000;

  const injectInitialEntry = options?.injectInitialEntry;
  const deduplicatePatches = options?.deduplicatePatches;

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
    if (!enabled || !endpoint) {
      // Close connection and reset state
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
      setIsInitialized(false);
      setError(null);
      dataRef.current = undefined;
      return;
    }

    // Initialize data
    if (!dataRef.current) {
      dataRef.current = initialData();

      // Inject initial entry if provided
      if (injectInitialEntry) {
        injectInitialEntry(dataRef.current);
      }
    }

    let cancelled = false;

    // Create WebSocket if it doesn't exist
    if (!wsRef.current) {
      // Reset finished flag for new connection
      finishedRef.current = false;

      void (async () => {
        try {
          const ws = await openLocalApiWebSocket(endpoint);

          if (cancelled) {
            ws.close();
            return;
          }

          ws.onopen = () => {
            setError(null);
            setIsConnected(true);
            // Reset backoff on successful connection
            retryAttemptsRef.current = 0;
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
              const idleMs = Date.now() - lastActivityRef.current;
              if (idleMs > IDLE_TIMEOUT_MS && wsRef.current === ws) {
                console.warn(
                  `[useJsonPatchWsStream] no activity for ${idleMs}ms, ` +
                    'forcing reconnect'
                );
                // Non-1000 close code so the reconnect logic in onclose runs.
                try {
                  ws.close(4000, 'idle-timeout');
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

                const current = dataRef.current;
                if (!filtered.length || !current) return;

                // Use Immer for structural sharing - only modified parts get new references
                const next = produce(current, (draft) => {
                  applyUpsertPatch(draft, filtered);
                });

                dataRef.current = next;
                setData(next);
              }

              // Handle Ready messages (initial data has been sent)
              if ('Ready' in msg) {
                initializedForEndpointRef.current = endpoint;
                setIsInitialized(true);
                setError(null);
              }

              // Handle finished messages ({finished: true})
              // Treat finished as terminal - do NOT reconnect
              if ('finished' in msg) {
                finishedRef.current = true;
                ws.close(1000, 'finished');
                wsRef.current = null;
                setIsConnected(false);
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
            // Only show error if we haven't received any data yet
            if (!dataRef.current && retryAttemptsRef.current > 6) {
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
          retryAttemptsRef.current += 1;
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
      dataRef.current = undefined;
      setData(undefined);
      setIsInitialized(false);
    };
  }, [
    endpoint,
    enabled,
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
    error,
  };
};
