import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { describe, it, expect, afterEach, vi } from 'vitest';
import React, { createContext } from 'react';
import {
  setLocalApiTransport,
  defaultTransport,
} from '@/shared/lib/localApiTransport';
import { useWorkspaces } from '@/shared/hooks/useWorkspaces';
import {
  useUiPreferencesStore,
  PERSIST_KEYS,
} from '@/shared/stores/useUiPreferencesStore';

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
  constructor(public url: string) {
    MockWebSocket.instances.push(this);
  }
  close() {
    this.closed = true;
    this.onclose?.({ code: 1000, wasClean: true } as CloseEvent);
  }
  send() {}
  static get aliveUrls(): string[] {
    return MockWebSocket.instances
      .filter((ws) => !ws.closed)
      .map((ws) => ws.url);
  }
}

function makeWrapper() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={qc}>{children}</QueryClientProvider>
  );
}

function setArchiveExpanded(expanded: boolean) {
  useUiPreferencesStore.setState((s) => ({
    expanded: {
      ...s.expanded,
      [PERSIST_KEYS.workspacesSidebarArchived]: expanded,
    },
  }));
}

afterEach(() => {
  MockWebSocket.instances = [];
  setLocalApiTransport(defaultTransport);
  // Reset only the slice this file mutated to avoid leaking state to other tests.
  useUiPreferencesStore.setState((s) => ({
    expanded: {
      ...s.expanded,
      [PERSIST_KEYS.workspacesSidebarArchived]: false,
    },
  }));
});

describe('useWorkspaces', () => {
  it('opens only the active WebSocket when archive section is collapsed', async () => {
    setLocalApiTransport({
      ...defaultTransport,
      openWebSocket: async (path) =>
        new MockWebSocket(path) as unknown as WebSocket,
    });

    setArchiveExpanded(false);

    const { unmount } = renderHook(() => useWorkspaces(), {
      wrapper: makeWrapper(),
    });
    try {
      await waitFor(() => {
        expect(
          MockWebSocket.aliveUrls.some((u) => u.includes('archived=false'))
        ).toBe(true);
      });

      const urls = MockWebSocket.aliveUrls;
      expect(urls.some((u) => u.includes('archived=true'))).toBe(false);
    } finally {
      unmount();
    }
  });

  it('opens the archived WebSocket when archive section is expanded', async () => {
    setLocalApiTransport({
      ...defaultTransport,
      openWebSocket: async (path) =>
        new MockWebSocket(path) as unknown as WebSocket,
    });

    setArchiveExpanded(true);

    const { unmount } = renderHook(() => useWorkspaces(), {
      wrapper: makeWrapper(),
    });
    try {
      await waitFor(() => {
        expect(
          MockWebSocket.aliveUrls.some((u) => u.includes('archived=true'))
        ).toBe(true);
      });
    } finally {
      unmount();
    }
  });
});
