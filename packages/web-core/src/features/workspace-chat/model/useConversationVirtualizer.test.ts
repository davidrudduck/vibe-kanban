import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import type { RefObject } from 'react';
import React from 'react';
import { useConversationVirtualizer } from './useConversationVirtualizer';

class MockResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}

if (typeof globalThis.ResizeObserver === 'undefined') {
  // jsdom does not provide ResizeObserver; install a no-op for TanStack Virtual.
  globalThis.ResizeObserver =
    MockResizeObserver as unknown as typeof ResizeObserver;
}

function makeContainer(clientHeight: number, scrollHeight: number) {
  const el = document.createElement('div');
  Object.defineProperty(el, 'clientHeight', {
    value: clientHeight,
    configurable: true,
  });
  Object.defineProperty(el, 'scrollHeight', {
    value: scrollHeight,
    configurable: true,
    writable: true,
  });
  Object.defineProperty(el, 'clientWidth', {
    value: 800,
    configurable: true,
  });
  el.scrollTop = 0;
  return el;
}

describe('useConversationVirtualizer — bottom-lock re-arm regression', () => {
  let container: HTMLDivElement;

  beforeEach(() => {
    container = makeContainer(500, 2000);
    document.body.appendChild(container);
  });

  afterEach(() => {
    container.remove();
  });

  it('re-arms the bottom lock when scrollTop reaches the bottom edge after scrollToTop releases it', () => {
    const ref: RefObject<HTMLDivElement | null> = { current: container };
    const { result } = renderHook(() =>
      useConversationVirtualizer({
        rows: [],
        totalRowCount: 0,
        scrollContainerRef: ref,
      })
    );

    // 1. Arm the lock by simulating scrollToBottom
    act(() => {
      result.current.scrollToBottom('auto');
    });
    expect(result.current.isBottomScrollCorrectionActive()).toBe(true);

    // 2. scrollToTop releases the lock
    act(() => {
      result.current.scrollToTop('auto');
    });
    expect(result.current.isBottomScrollCorrectionActive()).toBe(false);

    // 3. User manually scrolls back to bottom
    act(() => {
      container.scrollTop = container.scrollHeight - container.clientHeight; // 1500
      container.dispatchEvent(new Event('scroll'));
    });

    // 4. Bottom lock MUST be re-armed (regression assertion — FAILS today)
    expect(result.current.isBottomScrollCorrectionActive()).toBe(true);
  });
});

describe('useConversationVirtualizer — message-existence selectors', () => {
  it('hasNextUserMessage is false when no later user message exists', () => {
    const ref = { current: makeContainer(500, 2000) };
    const rows = [
      { isUserMessage: true },
      { isUserMessage: false },
      { isUserMessage: false },
    ];
    const { result } = renderHook(() =>
      useConversationVirtualizer({
        scrollContainerRef: ref as React.RefObject<HTMLElement>,
        rows,
      } as Parameters<typeof useConversationVirtualizer>[0])
    );
    expect(result.current.hasNextUserMessage()).toBe(false);
    expect(result.current.hasPreviousUserMessage()).toBe(false);
  });

  it('hasPreviousUserMessage is true when an earlier user message exists', () => {
    const ref = { current: makeContainer(500, 2000) };
    const rows = [
      { isUserMessage: true },
      { isUserMessage: false },
      { isUserMessage: true },
      { isUserMessage: false },
    ];
    const { result } = renderHook(() =>
      useConversationVirtualizer({
        scrollContainerRef: ref as React.RefObject<HTMLElement>,
        rows,
      } as Parameters<typeof useConversationVirtualizer>[0])
    );
    // From the last row, both should exist.
    expect(result.current.hasPreviousUserMessage()).toBe(true);
    expect(result.current.hasNextUserMessage()).toBe(false);
  });
});
