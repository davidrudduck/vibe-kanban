import { useCallback, useEffect, useRef, useState } from 'react';
import type { RefObject } from 'react';
import type { ConversationListHandle } from '../ui/ConversationListContainer';

export interface ConversationNavState {
  isAtBottom: boolean;
  isAtTop: boolean;
  hasPreviousUserMessage: boolean;
  hasNextUserMessage: boolean;
}

export interface ConversationNavController {
  // State
  isAtBottom: boolean;
  isAtTop: boolean;
  isAtBottomRef: RefObject<boolean>;
  hasPreviousUserMessage: boolean;
  hasNextUserMessage: boolean;
  // Callbacks for ConversationList
  onAtBottomChange: (atBottom: boolean) => void;
  onAtTopChange: (atTop: boolean) => void;
  // Handlers for nav overlay
  onScrollToTop: () => void;
  onScrollToBottom: () => void;
  onScrollToPreviousMessage: () => void;
  onScrollToNextMessage: () => void;
  onScrollToUserMessage: (patchKey: string) => void;
  getActiveTurnPatchKey: () => string | null;
}

/**
 * Bookkeeping for the conversation navigation overlay (top / prev user msg /
 * next user msg / bottom). Single source of truth for the four shells that
 * render the overlay; encapsulates `isAtBottom` / `isAtTop` state, the
 * `isAtBottomRef` mirror used by `ResizeObserver` callbacks, the existence
 * selectors used to gate the prev/next buttons, and the wired-up scroll
 * handlers that delegate to `ConversationListHandle`.
 */
export function useConversationNavController(
  ref: RefObject<ConversationListHandle>
): ConversationNavController {
  const [isAtBottom, setIsAtBottom] = useState(true);
  const [isAtTop, setIsAtTop] = useState(true);
  const [hasPreviousUserMessage, setHasPreviousUserMessage] = useState(false);
  const [hasNextUserMessage, setHasNextUserMessage] = useState(false);
  const isAtBottomRef = useRef(true);

  useEffect(() => {
    isAtBottomRef.current = isAtBottom;
  }, [isAtBottom]);

  const refreshExistence = useCallback(() => {
    setHasPreviousUserMessage(ref.current?.hasPreviousUserMessage() ?? false);
    setHasNextUserMessage(ref.current?.hasNextUserMessage() ?? false);
  }, [ref]);

  const onAtBottomChange = useCallback(
    (atBottom: boolean) => {
      isAtBottomRef.current = atBottom;
      setIsAtBottom(atBottom);
      refreshExistence();
    },
    [refreshExistence]
  );

  const onAtTopChange = useCallback(
    (atTop: boolean) => {
      setIsAtTop(atTop);
      refreshExistence();
    },
    [refreshExistence]
  );

  const onScrollToTop = useCallback(() => {
    ref.current?.scrollToTop('auto');
  }, [ref]);

  const onScrollToBottom = useCallback(() => {
    ref.current?.scrollToBottom('auto');
  }, [ref]);

  const onScrollToPreviousMessage = useCallback(() => {
    ref.current?.scrollToPreviousUserMessage();
  }, [ref]);

  const onScrollToNextMessage = useCallback(() => {
    ref.current?.scrollToNextUserMessage();
  }, [ref]);

  const onScrollToUserMessage = useCallback(
    (patchKey: string) => {
      ref.current?.scrollToEntryByPatchKey(patchKey);
    },
    [ref]
  );

  const getActiveTurnPatchKey = useCallback(() => {
    return ref.current?.getVisibleUserMessagePatchKey() ?? null;
  }, [ref]);

  return {
    isAtBottom,
    isAtTop,
    isAtBottomRef,
    hasPreviousUserMessage,
    hasNextUserMessage,
    onAtBottomChange,
    onAtTopChange,
    onScrollToTop,
    onScrollToBottom,
    onScrollToPreviousMessage,
    onScrollToNextMessage,
    onScrollToUserMessage,
    getActiveTurnPatchKey,
  };
}
