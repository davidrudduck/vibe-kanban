import { describe, it, expect, vi, beforeEach } from 'vitest';
import { streamJsonPatchEntries } from './streamJsonPatchEntries';

// Mock the localApiTransport module
vi.mock('@/shared/lib/localApiTransport', () => ({
  openLocalApiWebSocket: vi.fn(),
}));

import { openLocalApiWebSocket } from '@/shared/lib/localApiTransport';

// Provide requestAnimationFrame / cancelAnimationFrame stubs for Node environment.
// Use synchronous execution so flush() runs immediately during tests.
if (typeof globalThis.requestAnimationFrame === 'undefined') {
  let rafId = 0;
  globalThis.requestAnimationFrame = (cb: FrameRequestCallback): number => {
    const id = ++rafId;
    // Execute synchronously so batched ops are applied immediately in tests
    cb(0);
    return id;
  };
  globalThis.cancelAnimationFrame = (_id: number): void => {
    // no-op — sync execution means the callback already ran
  };
}

type EventMap = {
  open: (() => void)[];
  message: ((e: { data: string }) => void)[];
  error: ((e: unknown) => void)[];
  close: (() => void)[];
};

/**
 * A minimal mock WebSocket class that captures event listeners
 * and allows the test to trigger them manually.
 */
class MockWebSocket {
  private listeners: EventMap = {
    open: [],
    message: [],
    error: [],
    close: [],
  };

  addEventListener(event: string, handler: (...args: unknown[]) => void) {
    const key = event as keyof EventMap;
    if (this.listeners[key]) {
      (this.listeners[key] as ((...args: unknown[]) => void)[]).push(handler);
    }
  }

  private _closed = false;

  close() {
    // Note: fires close event synchronously; real WebSocket fires asynchronously.
    if (this._closed) return;
    this._closed = true;
    this.triggerClose();
  }

  triggerOpen() {
    this.listeners.open.forEach((h) => h());
  }

  triggerMessage(data: unknown) {
    const event = { data: JSON.stringify(data) };
    this.listeners.message.forEach((h) => h(event));
  }

  triggerError(err: unknown = new Event('error')) {
    this.listeners.error.forEach((h) => h(err));
  }

  triggerClose() {
    this.listeners.close.forEach((h) => h());
  }
}

const mockOpenWs = vi.mocked(openLocalApiWebSocket);

describe('streamJsonPatchEntries', () => {
  let mockWs: MockWebSocket;

  beforeEach(() => {
    vi.clearAllMocks();
    mockWs = new MockWebSocket();
    mockOpenWs.mockResolvedValue(mockWs as unknown as WebSocket);
  });

  it('calls onFinished and does NOT call onError when Finished frame received cleanly', async () => {
    const onFinished = vi.fn();
    const onError = vi.fn();

    streamJsonPatchEntries('/test', { onFinished, onError });

    // Wait for the async openLocalApiWebSocket promise to resolve
    await Promise.resolve();
    await Promise.resolve();

    const triggerCloseSpy = vi.spyOn(mockWs, 'triggerClose');

    mockWs.triggerOpen();
    mockWs.triggerMessage({ JsonPatch: [] });
    mockWs.triggerMessage({ finished: '' });
    // close event fires after ws.close() is called inside finished handler
    // MockWebSocket.close() calls triggerClose() directly

    expect(onFinished).toHaveBeenCalledOnce();
    expect(onError).not.toHaveBeenCalled();
    // Verify that ws.close() was actually called (and triggered the close handler)
    expect(triggerCloseSpy).toHaveBeenCalledOnce();
  });

  it('calls onError when WebSocket closes without Finished frame', async () => {
    const onError = vi.fn();

    streamJsonPatchEntries('/test', { onError });

    await Promise.resolve();
    await Promise.resolve();

    mockWs.triggerOpen();
    mockWs.triggerMessage({ JsonPatch: [] });
    // Close without sending Finished frame
    mockWs.triggerClose();

    expect(onError).toHaveBeenCalledOnce();
    expect(onError).toHaveBeenCalledWith(
      expect.objectContaining({ message: 'WebSocket closed without Finished frame' })
    );
  });

  it('does NOT call onError when controller.close() closes the socket (intentional close)', async () => {
    const onError = vi.fn();

    const controller = streamJsonPatchEntries('/test', { onError });

    await Promise.resolve();
    await Promise.resolve();

    mockWs.triggerOpen();

    // Intentionally close via controller
    controller.close();
    // triggerClose is called by MockWebSocket.close() inside controller.close() → ws.close()

    expect(onError).not.toHaveBeenCalled();
  });

  it('calls onError on WebSocket error event', async () => {
    const onError = vi.fn();

    streamJsonPatchEntries('/test', { onError });

    await Promise.resolve();
    await Promise.resolve();

    mockWs.triggerOpen();
    mockWs.triggerError(new Event('error'));

    expect(onError).toHaveBeenCalledOnce();
  });
});
