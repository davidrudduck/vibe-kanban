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

// Minimal WebSocket mock that tracks net-open sockets (opened and not closed).
// React StrictMode may open-then-immediately-close a socket on the first mount;
// we only care about sockets that survive (i.e. were not closed by cleanup).
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
  MockWebSocket.instances = [];
  setLocalApiTransport(defaultTransport);
});

describe('useJsonPatchWsStream', () => {
  it('opens exactly one WebSocket under React StrictMode', async () => {
    setLocalApiTransport({
      ...defaultTransport,
      openWebSocket: async (path) => {
        return new MockWebSocket(path) as unknown as WebSocket;
      },
    });

    renderHook(
      () =>
        useJsonPatchWsStream('/api/test-endpoint', true, () => ({} as object)),
      {
        wrapper: ({ children }) => (
          <StrictMode>{children}</StrictMode>
        ),
      }
    );

    // Flush microtasks (let the async IIFE in useEffect run)
    await act(async () => {
      await new Promise((r) => setTimeout(r, 50));
    });

    // The fix (await Promise.resolve() before new WebSocket) ensures StrictMode
    // cleanup cancels the first-mount socket before it's assigned; only the
    // second-mount socket survives. Both may be constructed, but only one stays
    // alive (not closed).
    expect(MockWebSocket.aliveUrls).toHaveLength(1);
    expect(MockWebSocket.aliveUrls[0]).toBe('/api/test-endpoint');
  });

  it('opens exactly one WebSocket without StrictMode (production behaviour)', async () => {
    setLocalApiTransport({
      ...defaultTransport,
      openWebSocket: async (path) => {
        return new MockWebSocket(path) as unknown as WebSocket;
      },
    });

    renderHook(() =>
      useJsonPatchWsStream('/api/test-endpoint', true, () => ({} as object))
    );

    await act(async () => {
      await new Promise((r) => setTimeout(r, 50));
    });

    expect(MockWebSocket.aliveUrls).toHaveLength(1);
  });
});
