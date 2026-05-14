import { renderHook, act } from '@testing-library/react';
import React, { StrictMode, createContext } from 'react';
import { describe, it, expect, afterEach, vi } from 'vitest';
import {
  setLocalApiTransport,
  defaultTransport,
} from '@/shared/lib/localApiTransport';
import { useJsonPatchWsStream } from '@/shared/hooks/useJsonPatchWsStream';

// createHmrContext writes to import.meta.hot.data which is undefined in jsdom.
// Mock the module so createHmrContext falls back to a plain createContext.
vi.mock('@/shared/lib/hmrContext', () => ({
  createHmrContext: <T,>(_key: string, defaultValue: T) =>
    createContext<T>(defaultValue),
}));

function setHidden(hidden: boolean) {
  Object.defineProperty(document, 'visibilityState', {
    configurable: true,
    get: () => (hidden ? 'hidden' : 'visible'),
  });
  Object.defineProperty(document, 'hidden', {
    configurable: true,
    get: () => hidden,
  });
  document.dispatchEvent(new Event('visibilitychange'));
}

// Minimal WebSocket mock that tracks all constructed sockets and their state.
// With the source-level fix (await Promise.resolve() inside openWebSocket), React
// StrictMode constructs TWO sockets: mount-1's socket is cancelled after construction,
// mount-2's socket survives. This is intentional — the deferred construction gives
// StrictMode's synchronous cleanup time to set cancelled=true before the TCP upgrade.
class MockWebSocket {
  static instances: MockWebSocket[] = [];
  onopen: ((e: Event) => void) | null = null;
  onmessage: ((e: MessageEvent) => void) | null = null;
  onerror: ((e: Event) => void) | null = null;
  onclose: ((e: CloseEvent) => void) | null = null;
  readyState: number = 0; // CONNECTING
  closed = false;

  constructor(public url: string) {
    MockWebSocket.instances.push(this);
  }

  open() {
    this.readyState = 1; // OPEN
    this.onopen?.({} as Event);
  }

  emit(data: unknown) {
    this.onmessage?.({ data: JSON.stringify(data) } as MessageEvent);
  }

  close(code = 1000) {
    this.closed = true;
    this.readyState = 3; // CLOSED
    this.onclose?.({ code, wasClean: code === 1000 } as CloseEvent);
  }

  send() {}

  static get aliveUrls(): string[] {
    return MockWebSocket.instances
      .filter((ws) => !ws.closed)
      .map((ws) => ws.url);
  }
}

afterEach(() => {
  vi.useRealTimers();
  MockWebSocket.instances = [];
  setLocalApiTransport(defaultTransport);
  // Reset visibility state in case a test left it hidden
  if (typeof document !== 'undefined' && document.hidden) {
    Object.defineProperty(document, 'visibilityState', {
      configurable: true,
      get: () => 'visible',
    });
    Object.defineProperty(document, 'hidden', {
      configurable: true,
      get: () => false,
    });
  }
});

describe('useJsonPatchWsStream', () => {
  it('opens exactly one WebSocket under React StrictMode', async () => {
    setLocalApiTransport({
      ...defaultTransport,
      openWebSocket: async (path) => {
        // Defer by one microtask like the real implementation, so StrictMode
        // cleanup can cancel before the WebSocket constructor is called
        await Promise.resolve();
        return new MockWebSocket(path) as unknown as WebSocket;
      },
    });

    renderHook(
      () =>
        useJsonPatchWsStream('/api/test-endpoint', true, () => ({}) as object),
      {
        wrapper: ({ children }) => <StrictMode>{children}</StrictMode>,
      }
    );

    // Flush microtasks (let the async IIFE in useEffect run)
    await act(async () => {
      await new Promise((r) => setTimeout(r, 50));
    });

    // The user-visible invariant: exactly one WebSocket survives (is not closed).
    // We do NOT assert how many were constructed — that is an implementation detail
    // of the microtask-defer approach and would break if a future fix avoids
    // constructing the first-mount socket entirely.
    expect(MockWebSocket.instances.filter((ws) => !ws.closed)).toHaveLength(1);
  });

  it('opens exactly one WebSocket without StrictMode (production behaviour)', async () => {
    setLocalApiTransport({
      ...defaultTransport,
      openWebSocket: async (path) => {
        return new MockWebSocket(path) as unknown as WebSocket;
      },
    });

    renderHook(() =>
      useJsonPatchWsStream('/api/test-endpoint', true, () => ({}) as object)
    );

    await act(async () => {
      await new Promise((r) => setTimeout(r, 50));
    });

    expect(MockWebSocket.aliveUrls).toHaveLength(1);
  });

  it('preserves received data across reconnect attempts', async () => {
    vi.useFakeTimers();
    setLocalApiTransport({
      ...defaultTransport,
      openWebSocket: async (path) =>
        new MockWebSocket(path) as unknown as WebSocket,
    });

    const { result } = renderHook(() =>
      useJsonPatchWsStream('/api/test-endpoint', true, () => ({
        execution_processes: {},
      }))
    );

    await act(async () => {
      await Promise.resolve();
    });

    const firstSocket = MockWebSocket.instances[0];
    firstSocket.open();

    await act(async () => {
      firstSocket.emit({
        JsonPatch: [
          {
            op: 'add',
            path: '/execution_processes/proc-1',
            value: { id: 'proc-1', status: 'running' },
          },
        ],
      });
      firstSocket.close(4000);
    });

    expect(result.current.data?.execution_processes['proc-1'].status).toBe(
      'running'
    );

    await act(async () => {
      vi.advanceTimersByTime(1000);
      await Promise.resolve();
    });

    expect(MockWebSocket.instances).toHaveLength(2);
    expect(result.current.data?.execution_processes['proc-1'].status).toBe(
      'running'
    );
  });

  it('closes the WebSocket when document is hidden past the grace period', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    setLocalApiTransport({
      ...defaultTransport,
      openWebSocket: async (path) =>
        new MockWebSocket(path) as unknown as WebSocket,
    });

    renderHook(() =>
      useJsonPatchWsStream('/api/test-endpoint', true, () => ({}) as object)
    );

    await act(async () => {
      await new Promise((r) => setTimeout(r, 50));
    });
    expect(MockWebSocket.aliveUrls).toHaveLength(1);

    act(() => {
      setHidden(true);
      vi.advanceTimersByTime(30_001);
    });

    expect(MockWebSocket.aliveUrls).toHaveLength(0);
  });

  it('reopens the WebSocket when document becomes visible again', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    setLocalApiTransport({
      ...defaultTransport,
      openWebSocket: async (path) =>
        new MockWebSocket(path) as unknown as WebSocket,
    });

    renderHook(() =>
      useJsonPatchWsStream('/api/test-endpoint', true, () => ({}) as object)
    );
    await act(async () => {
      await new Promise((r) => setTimeout(r, 50));
    });

    act(() => {
      setHidden(true);
      vi.advanceTimersByTime(30_001);
    });
    expect(MockWebSocket.aliveUrls).toHaveLength(0);

    act(() => {
      setHidden(false);
    });
    await act(async () => {
      await new Promise((r) => setTimeout(r, 50));
    });

    expect(MockWebSocket.aliveUrls).toHaveLength(1);
  });

  it('preserves data when tab is hidden past the grace window', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    setLocalApiTransport({
      ...defaultTransport,
      openWebSocket: async (path) =>
        new MockWebSocket(path) as unknown as WebSocket,
    });

    const { result } = renderHook(() =>
      useJsonPatchWsStream('/api/test-endpoint', true, () => ({
        items: {},
      }))
    );

    await act(async () => {
      await Promise.resolve();
    });
    const ws = MockWebSocket.instances[MockWebSocket.instances.length - 1];
    ws.open();

    // Send a patch so we have data
    await act(async () => {
      ws.emit({
        JsonPatch: [{ op: 'add', path: '/items/x', value: 'hello' }],
      });
    });
    expect(
      (result.current.data as { items: Record<string, string> })?.items?.x
    ).toBe('hello');

    // Hide the tab past grace window
    act(() => {
      setHidden(true);
      vi.advanceTimersByTime(30_001);
    });

    // WS closed — but data must still be present
    expect(MockWebSocket.aliveUrls).toHaveLength(0);
    expect(
      (result.current.data as { items: Record<string, string> })?.items?.x
    ).toBe('hello');
  });

  it('preserves data when endpoint query params change (stats_only toggle)', async () => {
    setLocalApiTransport({
      ...defaultTransport,
      openWebSocket: async (path) =>
        new MockWebSocket(path) as unknown as WebSocket,
    });

    const { result, rerender } = renderHook(
      ({ endpoint }: { endpoint: string }) =>
        useJsonPatchWsStream(endpoint, true, () => ({ items: {} })),
      { initialProps: { endpoint: '/api/diff?stats_only=false' } }
    );

    await act(async () => { await Promise.resolve(); });
    const ws1 = MockWebSocket.instances[MockWebSocket.instances.length - 1];
    ws1.open();

    await act(async () => {
      ws1.emit({
        JsonPatch: [{ op: 'add', path: '/items/file1', value: 'diff content' }],
      });
    });
    expect(
      (result.current.data as { items: Record<string, string> })?.items?.file1
    ).toBe('diff content');

    // Toggle to stats-only (same base path, different param)
    rerender({ endpoint: '/api/diff?stats_only=true' });
    await act(async () => { await Promise.resolve(); });

    // Data must be preserved — no blank flash
    expect(
      (result.current.data as { items: Record<string, string> })?.items?.file1
    ).toBe('diff content');
    // A new WS must have been created for the new endpoint
    expect(MockWebSocket.instances.length).toBeGreaterThan(1);
  });

  it('resets data when endpoint base path changes (different workspace)', async () => {
    setLocalApiTransport({
      ...defaultTransport,
      openWebSocket: async (path) =>
        new MockWebSocket(path) as unknown as WebSocket,
    });

    const { result, rerender } = renderHook(
      ({ endpoint }: { endpoint: string }) =>
        useJsonPatchWsStream(endpoint, true, () => ({ items: {} })),
      { initialProps: { endpoint: '/api/workspace-1/diff' } }
    );

    await act(async () => { await Promise.resolve(); });
    const ws1 = MockWebSocket.instances[MockWebSocket.instances.length - 1];
    ws1.open();

    await act(async () => {
      ws1.emit({
        JsonPatch: [{ op: 'add', path: '/items/file1', value: 'diff content' }],
      });
    });
    expect(
      (result.current.data as { items: Record<string, string> })?.items?.file1
    ).toBe('diff content');

    // Navigate to a different workspace (base path changes)
    rerender({ endpoint: '/api/workspace-2/diff' });
    await act(async () => { await Promise.resolve(); });

    // Data must be wiped — different workspace
    expect(result.current.data).toBeUndefined();
  });

  it('resets data (full teardown) when enabled becomes false', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    setLocalApiTransport({
      ...defaultTransport,
      openWebSocket: async (path) =>
        new MockWebSocket(path) as unknown as WebSocket,
    });

    const { result, rerender } = renderHook(
      ({ enabled }: { enabled: boolean }) =>
        useJsonPatchWsStream('/api/test-endpoint', enabled, () => ({
          items: {},
        })),
      { initialProps: { enabled: true } }
    );

    await act(async () => {
      await Promise.resolve();
    });
    const ws = MockWebSocket.instances[MockWebSocket.instances.length - 1];
    ws.open();

    await act(async () => {
      ws.emit({
        JsonPatch: [{ op: 'add', path: '/items/x', value: 'hello' }],
      });
    });
    expect(
      (result.current.data as { items: Record<string, string> })?.items?.x
    ).toBe('hello');

    // Explicitly disable (e.g. archived accordion collapses)
    rerender({ enabled: false });
    await act(async () => {
      await Promise.resolve();
    });

    // Data must be wiped on intentional disable
    expect(result.current.data).toBeUndefined();
    expect(result.current.isInitialized).toBe(false);
  });
});
