import { useEffect, useState, useRef, useReducer } from 'react';
import type { PatchType } from 'shared/types';
import { openLocalApiWebSocket } from '@/shared/lib/localApiTransport';

type LogEntry = Extract<PatchType, { type: 'STDOUT' } | { type: 'STDERR' }>;

type LogStreamState = {
  logs: LogEntry[];
  blockStartIndices: number[];
};

type LogStreamAction =
  | { type: 'ADD_ENTRY'; entry: LogEntry; isBoundary: boolean }
  | { type: 'REPLACE_FIRST'; entry: LogEntry }
  | { type: 'RESET' };

function logStreamReducer(
  state: LogStreamState,
  action: LogStreamAction
): LogStreamState {
  switch (action.type) {
    case 'ADD_ENTRY': {
      const nextIndex = state.logs.length;
      return {
        logs: [...state.logs, action.entry],
        blockStartIndices:
          action.isBoundary && nextIndex > 0
            ? [...state.blockStartIndices, nextIndex]
            : state.blockStartIndices,
      };
    }
    case 'REPLACE_FIRST':
      return { logs: [action.entry], blockStartIndices: [] };
    case 'RESET':
      return { logs: [], blockStartIndices: [] };
    default:
      return state;
  }
}

interface UseLogStreamResult {
  logs: LogEntry[];
  error: string | null;
  blockStartIndices: number[];
}

export const useLogStream = (processId: string): UseLogStreamResult => {
  const [state, dispatch] = useReducer(logStreamReducer, {
    logs: [],
    blockStartIndices: [],
  });
  const [error, setError] = useState<string | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const retryCountRef = useRef<number>(0);
  const retryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isIntentionallyClosed = useRef<boolean>(false);
  // Prevent reconnection after the server signals the stream is done
  const finishedRef = useRef<boolean>(false);
  // Track current processId to prevent stale WebSocket messages from contaminating logs
  const currentProcessIdRef = useRef<string>(processId);
  const pendingBlockBoundaryRef = useRef<boolean>(false);

  useEffect(() => {
    if (!processId) {
      return;
    }

    let cancelled = false;

    // Update the ref to track the current processId
    currentProcessIdRef.current = processId;

    // Clear logs when process changes
    dispatch({ type: 'RESET' });
    pendingBlockBoundaryRef.current = false;
    setError(null);
    finishedRef.current = false;

    const open = () => {
      // Don't reconnect if the stream already signalled finished
      if (finishedRef.current) {
        return;
      }

      // Capture processId at the time of opening the WebSocket
      const capturedProcessId = processId;
      void (async () => {
        try {
          // Transport defers WebSocket construction by one microtask; see localApiTransport.ts.
          const ws = await openLocalApiWebSocket(
            `/api/execution-processes/${processId}/raw-logs/ws`
          );

          if (cancelled || currentProcessIdRef.current !== capturedProcessId) {
            ws.close();
            return;
          }

          wsRef.current = ws;
          isIntentionallyClosed.current = false;

          // Track whether this is a reconnect so we can replace (not append)
          // logs on the first incoming message to avoid duplicates from
          // the server replaying history.
          const isReconnect = retryCountRef.current > 0;
          let pendingReplace = isReconnect;

          ws.onopen = () => {
            // Ignore if processId has changed since WebSocket was opened
            if (
              cancelled ||
              currentProcessIdRef.current !== capturedProcessId
            ) {
              ws.close();
              return;
            }
            setError(null);
            retryCountRef.current = 0;
            // Don't clear logs here — on reconnect the server replays
            // history, and clearing eagerly causes a flash if the
            // connection drops again before data arrives.
          };

          const addLogEntry = (entry: LogEntry) => {
            // Only add log entry if this WebSocket is still for the current process
            if (
              cancelled ||
              currentProcessIdRef.current !== capturedProcessId
            ) {
              return;
            }
            if (pendingReplace) {
              // First entry after reconnect: replace old logs to avoid
              // duplicates from the history replay.
              pendingReplace = false;
              pendingBlockBoundaryRef.current = false;
              dispatch({ type: 'REPLACE_FIRST', entry });
            } else {
              const isBoundary = pendingBlockBoundaryRef.current;
              pendingBlockBoundaryRef.current = false;
              dispatch({ type: 'ADD_ENTRY', entry, isBoundary });
            }
          };

          // Handle WebSocket messages
          ws.onmessage = (event) => {
            try {
              const data = JSON.parse(event.data);

              // Handle different message types based on LogMsg enum
              if ('JsonPatch' in data) {
                const patches = data.JsonPatch as Array<{ value?: PatchType }>;
                patches.forEach((patch) => {
                  const value = patch?.value;
                  if (!value || !value.type) return;

                  switch (value.type) {
                    case 'STDOUT':
                    case 'STDERR':
                      addLogEntry({ type: value.type, content: value.content });
                      break;
                    case 'NORMALIZED_ENTRY':
                      pendingBlockBoundaryRef.current = true;
                      break;
                    // Ignore other patch types (DIFF, etc.)
                    default:
                      break;
                  }
                });
              } else if (data.finished === true) {
                finishedRef.current = true;
                isIntentionallyClosed.current = true;
                ws.close();
              }
            } catch (e) {
              console.error('Failed to parse message:', e);
            }
          };

          ws.onerror = () => {
            // Don't set error here — onclose always fires after onerror
            // and handles retry logic. Setting error eagerly hides logs
            // that were already received.
          };

          ws.onclose = (event) => {
            // Don't retry for stale WebSocket connections
            if (
              cancelled ||
              currentProcessIdRef.current !== capturedProcessId
            ) {
              return;
            }
            // Only retry if the close was not intentional and not a normal closure
            if (!isIntentionallyClosed.current && event.code !== 1000) {
              const next = retryCountRef.current + 1;
              retryCountRef.current = next;
              if (next <= 6) {
                const delay = Math.min(1500, 250 * 2 ** (next - 1));
                retryTimerRef.current = setTimeout(() => open(), delay);
              } else {
                setError('Connection failed');
              }
            }
          };
        } catch (error) {
          if (cancelled || currentProcessIdRef.current !== capturedProcessId) {
            return;
          }
          const next = retryCountRef.current + 1;
          retryCountRef.current = next;
          if (next <= 6) {
            const delay = Math.min(1500, 250 * 2 ** (next - 1));
            retryTimerRef.current = setTimeout(() => open(), delay);
          } else {
            setError('Connection failed');
          }
        }
      })();
    };

    open();

    return () => {
      cancelled = true;
      if (wsRef.current) {
        isIntentionallyClosed.current = true;
        wsRef.current.close();
        wsRef.current = null;
      }
      if (retryTimerRef.current) {
        clearTimeout(retryTimerRef.current);
        retryTimerRef.current = null;
      }
    };
  }, [processId]);

  return {
    logs: state.logs,
    error,
    blockStartIndices: state.blockStartIndices,
  };
};
