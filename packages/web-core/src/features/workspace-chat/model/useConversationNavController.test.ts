import { afterEach, describe, it, expect, vi } from 'vitest';
import { cleanup, renderHook, act } from '@testing-library/react';
import type { RefObject } from 'react';
import { useConversationNavController } from './useConversationNavController';
import type { ConversationListHandle } from '../ui/ConversationListContainer';

// Vitest globals are disabled; testing-library's auto-cleanup hook does not
// register, so flush React trees explicitly between tests.
afterEach(cleanup);

function makeHandle(
  overrides: Partial<ConversationListHandle> = {}
): ConversationListHandle {
  return {
    scrollToTop: vi.fn(),
    scrollToBottom: vi.fn(),
    scrollToPreviousUserMessage: vi.fn(),
    scrollToNextUserMessage: vi.fn(),
    scrollToEntryByPatchKey: vi.fn(),
    getVisibleUserMessagePatchKey: vi.fn(() => 'patch-1'),
    hasPreviousUserMessage: vi.fn(() => true),
    hasNextUserMessage: vi.fn(() => false),
    adjustScrollBy: vi.fn(),
    getScrollElement: vi.fn(() => null),
    ...overrides,
  };
}

function makeRef(handle: ConversationListHandle): RefObject<ConversationListHandle> {
  return { current: handle };
}

describe('useConversationNavController', () => {
  it('forwards scroll handlers to the ref', () => {
    const handle = makeHandle();
    const ref = makeRef(handle);
    const { result } = renderHook(() => useConversationNavController(ref));

    act(() => result.current.onScrollToTop());
    expect(handle.scrollToTop).toHaveBeenCalledWith('auto');

    act(() => result.current.onScrollToBottom());
    expect(handle.scrollToBottom).toHaveBeenCalledWith('auto');

    act(() => result.current.onScrollToPreviousMessage());
    expect(handle.scrollToPreviousUserMessage).toHaveBeenCalledTimes(1);

    act(() => result.current.onScrollToNextMessage());
    expect(handle.scrollToNextUserMessage).toHaveBeenCalledTimes(1);

    act(() => result.current.onScrollToUserMessage('patch-7'));
    expect(handle.scrollToEntryByPatchKey).toHaveBeenCalledWith('patch-7');

    expect(result.current.getActiveTurnPatchKey()).toBe('patch-1');
    expect(handle.getVisibleUserMessagePatchKey).toHaveBeenCalled();
  });

  it('refreshes existence flags on edge changes', () => {
    const handle = makeHandle();
    const ref = makeRef(handle);
    const { result } = renderHook(() => useConversationNavController(ref));

    act(() => result.current.onAtBottomChange(false));
    expect(result.current.hasPreviousUserMessage).toBe(true);
    expect(result.current.hasNextUserMessage).toBe(false);
    expect(result.current.isAtBottom).toBe(false);

    act(() => result.current.onAtTopChange(false));
    expect(result.current.isAtTop).toBe(false);
    expect(handle.hasPreviousUserMessage).toHaveBeenCalled();
    expect(handle.hasNextUserMessage).toHaveBeenCalled();
  });

  it('keeps isAtBottomRef in sync', () => {
    const handle = makeHandle();
    const ref = makeRef(handle);
    const { result } = renderHook(() => useConversationNavController(ref));

    act(() => result.current.onAtBottomChange(false));
    expect(result.current.isAtBottomRef.current).toBe(false);

    act(() => result.current.onAtBottomChange(true));
    expect(result.current.isAtBottomRef.current).toBe(true);
  });

  it('returns null from getActiveTurnPatchKey when the ref reports no visible user message', () => {
    const handle = makeHandle({
      getVisibleUserMessagePatchKey: vi.fn(() => null),
    });
    const ref = makeRef(handle);
    const { result } = renderHook(() => useConversationNavController(ref));

    expect(result.current.getActiveTurnPatchKey()).toBeNull();
  });

  it('treats a null ref as no-op for handlers and returns false existence flags', () => {
    const ref: RefObject<ConversationListHandle> = { current: null };
    const { result } = renderHook(() => useConversationNavController(ref));

    // Handlers must not throw when the ref is unattached.
    act(() => result.current.onScrollToTop());
    act(() => result.current.onScrollToBottom());
    act(() => result.current.onScrollToPreviousMessage());
    act(() => result.current.onScrollToNextMessage());
    act(() => result.current.onScrollToUserMessage('patch-x'));

    act(() => result.current.onAtBottomChange(false));
    act(() => result.current.onAtTopChange(false));

    expect(result.current.hasPreviousUserMessage).toBe(false);
    expect(result.current.hasNextUserMessage).toBe(false);
    expect(result.current.getActiveTurnPatchKey()).toBeNull();
  });
});
