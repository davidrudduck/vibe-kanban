import { useEffect, useMemo, useRef, useState } from 'react';
import type { ExecutionLogEvent } from 'shared/types';
import { openLocalApiWebSocket } from '@/shared/lib/localApiTransport';

type WsReadyMsg = { Ready: true };
type WsFinishedMsg = { finished: true };
type WsEventMsg = { Event: ExecutionLogEvent };
type WsMsg = WsReadyMsg | WsFinishedMsg | WsEventMsg;

const activeStreams = new Map<string, symbol>();
const EMPTY_EVENTS: ExecutionLogEvent[] = [];

const eventIdNumber = (event: ExecutionLogEvent): number =>
  Number(event.id as unknown as number | bigint);

const buildLiveUrl = (executionProcessId: string, afterId: number) =>
  `/api/execution-processes/${executionProcessId}/events/live/ws?after_id=${afterId}`;

export interface UseExecutionEventStreamParams {
  executionProcessId: string | null | undefined;
  enabled: boolean;
  initialEvents?: ExecutionLogEvent[];
  retryBaseMs?: number;
}

export interface UseExecutionEventStreamResult {
  events: ExecutionLogEvent[];
  isConnected: boolean;
  isInitialized: boolean;
  isFinished: boolean;
  error: string | null;
}

export function useExecutionEventStream({
  executionProcessId,
  enabled,
  initialEvents = EMPTY_EVENTS,
  retryBaseMs = 1000,
}: UseExecutionEventStreamParams): UseExecutionEventStreamResult {
  const [events, setEvents] = useState<ExecutionLogEvent[]>(initialEvents);
  const [isConnected, setIsConnected] = useState(false);
  const [isInitialized, setIsInitialized] = useState(false);
  const [isFinished, setIsFinished] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const retryTimerRef = useRef<number | null>(null);
  const retryAttemptsRef = useRef(0);
  const [retryNonce, setRetryNonce] = useState(0);
  const finishedRef = useRef(false);
  const eventsByIdRef = useRef<Map<number, ExecutionLogEvent>>(new Map());
  const lastSeenEventIdRef = useRef(0);
  const ownerRef = useRef<symbol | null>(null);

  useEffect(() => {
    eventsByIdRef.current.clear();
    initialEvents.forEach((event) => {
      const id = eventIdNumber(event);
      eventsByIdRef.current.set(id, event);
      lastSeenEventIdRef.current = Math.max(lastSeenEventIdRef.current, id);
    });
    setEvents(
      [...eventsByIdRef.current.values()].sort(
        (a, b) => eventIdNumber(a) - eventIdNumber(b)
      )
    );
  }, [initialEvents]);

  useEffect(() => {
    if (!enabled || !executionProcessId || finishedRef.current) return;

    const owner = Symbol(executionProcessId);
    ownerRef.current = owner;
    const currentOwner = activeStreams.get(executionProcessId);
    if (currentOwner && currentOwner !== owner) {
      setError('Execution event stream already active');
      return;
    }
    activeStreams.set(executionProcessId, owner);

    let cancelled = false;

    const scheduleReconnect = () => {
      if (retryTimerRef.current || finishedRef.current || cancelled) return;
      const delay = Math.min(8000, retryBaseMs * 2 ** retryAttemptsRef.current);
      retryTimerRef.current = window.setTimeout(() => {
        retryTimerRef.current = null;
        setRetryNonce((value) => value + 1);
      }, delay);
    };

    void (async () => {
      try {
        const ws = await openLocalApiWebSocket(
          buildLiveUrl(executionProcessId, lastSeenEventIdRef.current)
        );
        if (cancelled) {
          ws.close();
          return;
        }

        ws.onopen = () => {
          setIsConnected(true);
          setError(null);
          retryAttemptsRef.current = 0;
        };

        ws.onmessage = (event) => {
          const msg: WsMsg = JSON.parse(event.data);
          if ('Ready' in msg) {
            setIsInitialized(true);
            return;
          }
          if ('finished' in msg) {
            finishedRef.current = true;
            setIsFinished(true);
            setIsConnected(false);
            ws.close(1000, 'finished');
            return;
          }
          if ('Event' in msg) {
            const id = eventIdNumber(msg.Event);
            if (eventsByIdRef.current.has(id)) return;
            eventsByIdRef.current.set(id, msg.Event);
            lastSeenEventIdRef.current = Math.max(
              lastSeenEventIdRef.current,
              id
            );
            setEvents(
              [...eventsByIdRef.current.values()].sort(
                (a, b) => eventIdNumber(a) - eventIdNumber(b)
              )
            );
          }
        };

        ws.onerror = () => {
          if (!eventsByIdRef.current.size) setError('Connection failed');
        };

        ws.onclose = (event) => {
          setIsConnected(false);
          wsRef.current = null;
          if (
            cancelled ||
            finishedRef.current ||
            (event.code === 1000 && event.wasClean)
          ) {
            return;
          }
          retryAttemptsRef.current += 1;
          scheduleReconnect();
        };

        wsRef.current = ws;
      } catch {
        if (!eventsByIdRef.current.size) setError('Connection failed');
        retryAttemptsRef.current += 1;
        scheduleReconnect();
      }
    })();

    return () => {
      cancelled = true;
      if (retryTimerRef.current) {
        window.clearTimeout(retryTimerRef.current);
        retryTimerRef.current = null;
      }
      if (wsRef.current) {
        wsRef.current.onopen = null;
        wsRef.current.onmessage = null;
        wsRef.current.onerror = null;
        wsRef.current.onclose = null;
        wsRef.current.close();
        wsRef.current = null;
      }
      if (activeStreams.get(executionProcessId) === ownerRef.current) {
        activeStreams.delete(executionProcessId);
      }
      setIsConnected(false);
    };
  }, [enabled, executionProcessId, retryBaseMs, retryNonce]);

  return useMemo(
    () => ({
      events,
      isConnected,
      isInitialized,
      isFinished,
      error,
    }),
    [events, isConnected, isInitialized, isFinished, error]
  );
}
