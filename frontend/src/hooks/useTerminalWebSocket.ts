import { useEffect, useRef, useCallback, useState } from 'react';
import type { TerminalMessage } from 'shared/types';

export type ConnectionState = 'connecting' | 'connected' | 'disconnected' | 'error';

export interface UseTerminalWebSocketOptions {
  sessionId: string | null;
  onOutput?: (data: string) => void;
  onExit?: (code: number | null) => void;
  onError?: (message: string) => void;
  onConnectionChange?: (state: ConnectionState) => void;
}

export interface UseTerminalWebSocketResult {
  sendInput: (data: string) => void;
  sendResize: (cols: number, rows: number) => void;
  connectionState: ConnectionState;
  reconnect: () => void;
}

export function useTerminalWebSocket({
  sessionId,
  onOutput,
  onExit,
  onError,
  onConnectionChange,
}: UseTerminalWebSocketOptions): UseTerminalWebSocketResult {
  const wsRef = useRef<WebSocket | null>(null);
  const [connectionState, setConnectionState] = useState<ConnectionState>('disconnected');
  const retryCountRef = useRef<number>(0);
  const retryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isIntentionallyClosed = useRef<boolean>(false);

  const updateConnectionState = useCallback(
    (state: ConnectionState) => {
      setConnectionState(state);
      onConnectionChange?.(state);
    },
    [onConnectionChange]
  );

  const connect = useCallback(() => {
    if (!sessionId) return;

    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const host = window.location.host;
    const ws = new WebSocket(`${protocol}//${host}/api/terminal/ws/${sessionId}`);

    wsRef.current = ws;
    isIntentionallyClosed.current = false;
    updateConnectionState('connecting');

    ws.onopen = () => {
      updateConnectionState('connected');
      retryCountRef.current = 0;
    };

    ws.onmessage = (event) => {
      try {
        const msg = JSON.parse(event.data) as TerminalMessage;
        switch (msg.type) {
          case 'output':
            onOutput?.(msg.data);
            break;
          case 'exit':
            onExit?.(msg.code);
            break;
          case 'error':
            onError?.(msg.message);
            break;
          case 'input':
          case 'resize':
            // These are client-to-server messages, ignore if received from server
            break;
        }
      } catch (e) {
        console.error('Failed to parse terminal message:', e);
      }
    };

    ws.onerror = () => {
      updateConnectionState('error');
    };

    ws.onclose = (event) => {
      if (isIntentionallyClosed.current) {
        updateConnectionState('disconnected');
        return;
      }

      // Retry connection with exponential backoff
      if (event.code !== 1000) {
        const next = retryCountRef.current + 1;
        retryCountRef.current = next;
        if (next <= 6) {
          const delay = Math.min(3000, 250 * 2 ** (next - 1));
          updateConnectionState('connecting');
          retryTimerRef.current = setTimeout(() => connect(), delay);
        } else {
          updateConnectionState('error');
        }
      } else {
        updateConnectionState('disconnected');
      }
    };
  }, [sessionId, onOutput, onExit, onError, updateConnectionState]);

  const disconnect = useCallback(() => {
    if (retryTimerRef.current) {
      clearTimeout(retryTimerRef.current);
      retryTimerRef.current = null;
    }
    if (wsRef.current) {
      isIntentionallyClosed.current = true;
      wsRef.current.close();
      wsRef.current = null;
    }
  }, []);

  useEffect(() => {
    if (sessionId) {
      connect();
    }
    return () => {
      disconnect();
    };
  }, [sessionId, connect, disconnect]);

  const sendInput = useCallback((data: string) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      const msg: TerminalMessage = { type: 'input', data };
      wsRef.current.send(JSON.stringify(msg));
    }
  }, []);

  const sendResize = useCallback((cols: number, rows: number) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      const msg: TerminalMessage = { type: 'resize', cols, rows };
      wsRef.current.send(JSON.stringify(msg));
    }
  }, []);

  const reconnect = useCallback(() => {
    disconnect();
    retryCountRef.current = 0;
    connect();
  }, [disconnect, connect]);

  return {
    sendInput,
    sendResize,
    connectionState,
    reconnect,
  };
}
