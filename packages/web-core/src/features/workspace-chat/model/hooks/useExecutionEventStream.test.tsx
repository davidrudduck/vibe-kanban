import { cleanup, renderHook, act } from '@testing-library/react';
import { createContext } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  defaultTransport,
  setLocalApiTransport,
} from '@/shared/lib/localApiTransport';
import { useExecutionEventStream } from './useExecutionEventStream';

vi.mock('@/shared/lib/hmrContext', () => ({
  createHmrContext: <T,>(_key: string, defaultValue: T) =>
    createContext<T>(defaultValue),
}));

class MockWebSocket {
  static instances: MockWebSocket[] = [];
  onopen: ((e: Event) => void) | null = null;
  onmessage: ((e: MessageEvent) => void) | null = null;
  onerror: ((e: Event) => void) | null = null;
  onclose: ((e: CloseEvent) => void) | null = null;
  closed = false;
  sent: string[] = [];

  constructor(public url: string) {
    MockWebSocket.instances.push(this);
  }

  open() {
    this.onopen?.({} as Event);
  }

  emit(data: unknown) {
    this.onmessage?.({ data: JSON.stringify(data) } as MessageEvent);
  }

  close(code = 1000) {
    this.closed = true;
    this.onclose?.({ code, wasClean: code === 1000 } as CloseEvent);
  }

  send(value: string) {
    this.sent.push(value);
  }
}

const makeEvent = (id: number, text: string) => ({
  id,
  execution_id: 'exec-1',
  source: 'test',
  source_event_id: `event-${id}`,
  event_type: 'raw_stdout',
  payload_json: { text },
  created_at: '2026-05-13T00:00:00Z',
});

const waitForSocket = async () => {
  await act(async () => {
    await Promise.resolve();
  });
  const socket = MockWebSocket.instances.at(-1);
  if (!socket) throw new Error('socket was not opened');
  return socket;
};

afterEach(() => {
  MockWebSocket.instances = [];
  setLocalApiTransport(defaultTransport);
  cleanup();
});

describe('useExecutionEventStream', () => {
  it('deduplicates live events by durable id', async () => {
    setLocalApiTransport({
      ...defaultTransport,
      openWebSocket: async (path) =>
        new MockWebSocket(path) as unknown as WebSocket,
    });

    const { result } = renderHook(() =>
      useExecutionEventStream({ executionProcessId: 'exec-1', enabled: true })
    );
    const socket = await waitForSocket();

    await act(async () => {
      socket.open();
      socket.emit({ Ready: true });
      socket.emit({ Event: makeEvent(1, 'one') });
      socket.emit({ Event: makeEvent(1, 'one duplicate') });
    });

    expect(result.current.events.map((event) => Number(event.id))).toEqual([1]);
    expect(result.current.events[0].payload_json).toEqual({ text: 'one' });
  });

  it('reconnects with the last seen event id without clearing events', async () => {
    setLocalApiTransport({
      ...defaultTransport,
      openWebSocket: async (path) =>
        new MockWebSocket(path) as unknown as WebSocket,
    });

    const { result } = renderHook(() =>
      useExecutionEventStream({
        executionProcessId: 'exec-1',
        enabled: true,
        retryBaseMs: 1,
      })
    );
    const socket = await waitForSocket();

    await act(async () => {
      socket.open();
      socket.emit({ Event: makeEvent(5, 'five') });
      socket.close(4000);
    });

    expect(result.current.events.map((event) => Number(event.id))).toEqual([5]);

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 5));
    });

    expect(MockWebSocket.instances[1].url).toContain('after_id=5');
    expect(result.current.events.map((event) => Number(event.id))).toEqual([5]);
  });

  it('treats finished as terminal', async () => {
    setLocalApiTransport({
      ...defaultTransport,
      openWebSocket: async (path) =>
        new MockWebSocket(path) as unknown as WebSocket,
    });

    const { result } = renderHook(() =>
      useExecutionEventStream({ executionProcessId: 'exec-1', enabled: true })
    );
    const socket = await waitForSocket();

    await act(async () => {
      socket.open();
      socket.emit({ Event: makeEvent(1, 'one') });
      socket.emit({ finished: true });
    });

    expect(result.current.isFinished).toBe(true);
    expect(MockWebSocket.instances).toHaveLength(1);
  });
});
