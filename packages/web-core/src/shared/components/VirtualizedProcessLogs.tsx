import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from 'react';
import { useTranslation } from 'react-i18next';
import {
  DataWithScrollModifier,
  VirtuosoMessageList,
  VirtuosoMessageListLicense,
  VirtuosoMessageListMethods,
  VirtuosoMessageListProps,
} from '@virtuoso.dev/message-list';
import { WarningCircleIcon } from '@phosphor-icons/react/dist/ssr';
import RawLogText from '@/shared/components/RawLogText';
import {
  INITIAL_TOP_ITEM,
  InitialDataScrollModifier,
  ScrollToBottomModifier as ScrollToLastItem,
} from '@/shared/lib/virtuoso-modifiers';
import type { PatchType } from 'shared/types';

export type LogEntry = Extract<
  PatchType,
  { type: 'STDOUT' } | { type: 'STDERR' }
>;

export interface LogViewerHandle {
  scrollToTop: () => void;
  scrollToBottom: () => void;
  scrollToPrevBlock: () => void;
  scrollToNextBlock: () => void;
}

export interface VirtualizedProcessLogsProps {
  logs: LogEntry[];
  error: string | null;
  searchQuery: string;
  matchIndices: number[];
  currentMatchIndex: number;
  blockStartIndices?: number[];
  onScrollPositionChange?: (position: {
    isAtTop: boolean;
    isAtBottom: boolean;
  }) => void;
}

type LogEntryWithKey = LogEntry & { key: string; originalIndex: number };

interface SearchContext {
  searchQuery: string;
  matchIndices: number[];
  currentMatchIndex: number;
}

const computeItemKey: VirtuosoMessageListProps<
  LogEntryWithKey,
  SearchContext
>['computeItemKey'] = ({ data }) => data.key;

const ItemContent: VirtuosoMessageListProps<
  LogEntryWithKey,
  SearchContext
>['ItemContent'] = ({ data, context }) => {
  const isMatch = context.matchIndices.includes(data.originalIndex);
  const isCurrentMatch =
    context.matchIndices[context.currentMatchIndex] === data.originalIndex;

  return (
    <RawLogText
      content={data.content}
      channel={data.type === 'STDERR' ? 'stderr' : 'stdout'}
      className="text-sm px-4 py-1"
      linkifyUrls
      searchQuery={isMatch ? context.searchQuery : undefined}
      isCurrentMatch={isCurrentMatch}
    />
  );
};

export const VirtualizedProcessLogs = forwardRef<
  LogViewerHandle,
  VirtualizedProcessLogsProps
>(function VirtualizedProcessLogs(
  {
    logs,
    error,
    searchQuery,
    matchIndices,
    currentMatchIndex,
    blockStartIndices = [],
    onScrollPositionChange,
  },
  ref
) {
  const { t } = useTranslation('tasks');
  const [channelData, setChannelData] =
    useState<DataWithScrollModifier<LogEntryWithKey> | null>(null);
  const messageListRef = useRef<VirtuosoMessageListMethods<
    LogEntryWithKey,
    SearchContext
  > | null>(null);
  const hasInitializedRef = useRef(false);
  const prevCurrentMatchRef = useRef<number | undefined>(undefined);
  const isAtBottomRef = useRef(true);
  const blockCursorRef = useRef(0);
  const isAtTopRef = useRef(true);
  const lastCommitTimeRef = useRef(0);
  const totalItemsRef = useRef(0);

  const scrollToTop = useCallback(() => {
    messageListRef.current?.scrollToItem({
      index: 0,
      align: 'start',
      behavior: 'smooth',
    });
  }, []);

  const scrollToBottom = useCallback(() => {
    messageListRef.current?.scrollToItem({
      index: 'LAST',
      align: 'end',
      behavior: 'smooth',
    });
  }, []);

  const syncCursorToViewport = useCallback(() => {
    if (blockStartIndices.length === 0) return;
    const location = messageListRef.current?.getScrollLocation();
    if (!location) return;
    const { listOffset, scrollHeight } = location;
    const fraction = scrollHeight > 0 ? listOffset / scrollHeight : 0;
    const approxItemIndex = Math.round(fraction * totalItemsRef.current);
    let lo = 0;
    let hi = blockStartIndices.length - 1;
    while (lo < hi) {
      const mid = Math.ceil((lo + hi) / 2);
      if (blockStartIndices[mid] <= approxItemIndex) lo = mid;
      else hi = mid - 1;
    }
    blockCursorRef.current = lo;
  }, [blockStartIndices]);

  const scrollToPrevBlock = useCallback(() => {
    if (blockStartIndices.length === 0) {
      messageListRef.current?.scrollToItem({
        index: 0,
        align: 'start',
        behavior: 'smooth',
      });
      return;
    }
    syncCursorToViewport();
    blockCursorRef.current = Math.max(0, blockCursorRef.current - 1);
    messageListRef.current?.scrollToItem({
      index: blockStartIndices[blockCursorRef.current],
      align: 'start',
      behavior: 'smooth',
    });
  }, [blockStartIndices, syncCursorToViewport]);

  const scrollToNextBlock = useCallback(() => {
    if (
      blockStartIndices.length === 0 ||
      blockCursorRef.current >= blockStartIndices.length - 1
    ) {
      messageListRef.current?.scrollToItem({
        index: 'LAST',
        align: 'end',
        behavior: 'smooth',
      });
      return;
    }
    syncCursorToViewport();
    blockCursorRef.current = Math.min(
      blockStartIndices.length - 1,
      blockCursorRef.current + 1
    );
    messageListRef.current?.scrollToItem({
      index: blockStartIndices[blockCursorRef.current],
      align: 'start',
      behavior: 'smooth',
    });
  }, [blockStartIndices, syncCursorToViewport]);

  useImperativeHandle(
    ref,
    () => ({
      scrollToTop,
      scrollToBottom,
      scrollToPrevBlock,
      scrollToNextBlock,
    }),
    [scrollToTop, scrollToBottom, scrollToPrevBlock, scrollToNextBlock]
  );

    const scrollToTop = useCallback(() => {
      messageListRef.current?.scrollToItem({ index: 0, align: 'start', behavior: 'smooth' });
    }, []);

    totalItemsRef.current = logsWithKeys.length;

    // Initial load: fire immediately — bypasses the per-entry debounce reset cascade
    if (!hasInitializedRef.current && logs.length > 0) {
      hasInitializedRef.current = true;
      lastCommitTimeRef.current = Date.now();
      setChannelData({
        data: logsWithKeys,
        scrollModifier: InitialDataScrollModifier,
      });
      return;
    }

    const now = Date.now();
    const timeSinceLastCommit = now - lastCommitTimeRef.current;

    // Max-wait: if it's been >=500ms since last commit, fire immediately to avoid starvation
    if (timeSinceLastCommit >= 500) {
      lastCommitTimeRef.current = now;
      const scrollModifier = isAtBottomRef.current ? ScrollToLastItem : null;
      if (scrollModifier) {
        setChannelData({ data: logsWithKeys, scrollModifier });
      } else {
        setChannelData({ data: logsWithKeys });
      }
      return;
    }

    // Otherwise debounce to batch rapid appends
    const timeoutId = setTimeout(() => {
      lastCommitTimeRef.current = Date.now();
      const scrollModifier = isAtBottomRef.current ? ScrollToLastItem : null;
      if (scrollModifier) {
        setChannelData({ data: logsWithKeys, scrollModifier });
      } else {
        setChannelData({ data: logsWithKeys });
      }
      syncCursorToViewport();
      blockCursorRef.current = Math.max(0, blockCursorRef.current - 1);
      messageListRef.current?.scrollToItem({
        index: blockStartIndices[blockCursorRef.current],
        align: 'start',
        behavior: 'smooth',
      });
    }, [blockStartIndices, syncCursorToViewport]);

    const scrollToNextBlock = useCallback(() => {
      if (blockStartIndices.length === 0 || blockCursorRef.current >= blockStartIndices.length - 1) {
        messageListRef.current?.scrollToItem({ index: 'LAST', align: 'end', behavior: 'smooth' });
        return;
      }
      syncCursorToViewport();
      blockCursorRef.current = Math.min(blockStartIndices.length - 1, blockCursorRef.current + 1);
      messageListRef.current?.scrollToItem({
        index: blockStartIndices[blockCursorRef.current],
        align: 'start',
        behavior: 'smooth',
      });
    }, [blockStartIndices, syncCursorToViewport]);

    useImperativeHandle(ref, () => ({
      scrollToTop,
      scrollToBottom,
      scrollToPrevBlock,
      scrollToNextBlock,
    }), [scrollToTop, scrollToBottom, scrollToPrevBlock, scrollToNextBlock]);

    useEffect(() => {
      const logsWithKeys: LogEntryWithKey[] = logs.map((entry, index) => ({
        ...entry,
        key: `log-${index}`,
        originalIndex: index,
      }));

      totalItemsRef.current = logsWithKeys.length;

      // Initial load: fire immediately — bypasses the per-entry debounce reset cascade
      if (!hasInitializedRef.current && logs.length > 0) {
        hasInitializedRef.current = true;
        lastCommitTimeRef.current = Date.now();
        setChannelData({ data: logsWithKeys, scrollModifier: InitialDataScrollModifier });
        return;
      }

      const now = Date.now();
      const timeSinceLastCommit = now - lastCommitTimeRef.current;

      // Max-wait: if it's been >=500ms since last commit, fire immediately to avoid starvation
      if (timeSinceLastCommit >= 500) {
        lastCommitTimeRef.current = now;
        const scrollModifier = isAtBottomRef.current ? ScrollToLastItem : null;
        if (scrollModifier) {
          setChannelData({ data: logsWithKeys, scrollModifier });
        } else {
          setChannelData({ data: logsWithKeys });
        }
        return;
      }

      // Otherwise debounce to batch rapid appends
      const timeoutId = setTimeout(() => {
        lastCommitTimeRef.current = Date.now();
        const scrollModifier = isAtBottomRef.current ? ScrollToLastItem : null;
        if (scrollModifier) {
          setChannelData({ data: logsWithKeys, scrollModifier });
        } else {
          setChannelData({ data: logsWithKeys });
        }
      }, 100);

      return () => clearTimeout(timeoutId);
    }, [logs]);

    // Scroll to current match when it changes
    useEffect(() => {
      if (
        matchIndices.length > 0 &&
        currentMatchIndex >= 0 &&
        currentMatchIndex !== prevCurrentMatchRef.current
      ) {
        const logIndex = matchIndices[currentMatchIndex];
        messageListRef.current?.scrollToItem({
          index: logIndex,
          align: 'center',
          behavior: 'smooth',
        });
        prevCurrentMatchRef.current = currentMatchIndex;
      }
    }, [currentMatchIndex, matchIndices]);

    if (logs.length === 0 && !error) {
      return (
        <div className="h-full flex items-center justify-center">
          <p className="text-center text-muted-foreground text-sm">
            {t('processes.noLogsAvailable')}
          </p>
        </div>
      );
    }

    if (error && logs.length === 0) {
      return (
        <div className="h-full flex items-center justify-center">
          <p className="text-center text-destructive text-sm">
            <WarningCircleIcon className="size-icon-base inline mr-2" />
            {error}
          </p>
        </div>
      );
    }

    const context: SearchContext = {
      searchQuery,
      matchIndices,
      currentMatchIndex,
    };

    return (
      <div className="virtuoso-license-wrapper h-full overflow-hidden">
        <VirtuosoMessageListLicense
          licenseKey={import.meta.env.VITE_PUBLIC_REACT_VIRTUOSO_LICENSE_KEY}
        >
          <VirtuosoMessageList<LogEntryWithKey, SearchContext>
            ref={messageListRef}
            className="h-full"
            data={channelData}
            context={context}
            initialLocation={INITIAL_TOP_ITEM}
            onScroll={(location) => {
              // Capture previous values before mutating refs (for transition detection)
              const prevAtBottom = isAtBottomRef.current;
              const prevAtTop = isAtTopRef.current;
              isAtBottomRef.current = location.isAtBottom;
              // Detect top by checking if the scroll element is at or near 0
              const scroller = messageListRef.current?.scrollerElement();
              const atTop = scroller ? scroller.scrollTop <= 0 : false;
              isAtTopRef.current = atTop;
              if (location.isAtBottom) {
                // Reset cursor to last block when reaching bottom
                blockCursorRef.current = blockStartIndices.length > 0 ? blockStartIndices.length - 1 : 0;
              }
              // Only notify parent on edge transitions to avoid re-rendering on every scroll pixel
              if (atTop !== prevAtTop || location.isAtBottom !== prevAtBottom) {
                onScrollPositionChange?.({ isAtTop: atTop, isAtBottom: location.isAtBottom });
              }
            }}
            computeItemKey={computeItemKey}
            ItemContent={ItemContent}
          />
        </VirtuosoMessageListLicense>
      </div>
    );
  }

  if (error && logs.length === 0) {
    return (
      <div className="h-full flex items-center justify-center">
        <p className="text-center text-destructive text-sm">
          <WarningCircleIcon className="size-icon-base inline mr-2" />
          {error}
        </p>
      </div>
    );
  }

  const context: SearchContext = {
    searchQuery,
    matchIndices,
    currentMatchIndex,
  };

  return (
    <div className="virtuoso-license-wrapper h-full overflow-hidden">
      <VirtuosoMessageListLicense
        licenseKey={import.meta.env.VITE_PUBLIC_REACT_VIRTUOSO_LICENSE_KEY}
      >
        <VirtuosoMessageList<LogEntryWithKey, SearchContext>
          ref={messageListRef}
          className="h-full"
          data={channelData}
          context={context}
          initialLocation={INITIAL_TOP_ITEM}
          onScroll={(location) => {
            // Capture previous values before mutating refs (for transition detection)
            const prevAtBottom = isAtBottomRef.current;
            const prevAtTop = isAtTopRef.current;
            isAtBottomRef.current = location.isAtBottom;
            // Detect top by checking if the scroll element is at or near 0
            const scroller = messageListRef.current?.scrollerElement();
            const atTop = scroller ? scroller.scrollTop <= 0 : false;
            isAtTopRef.current = atTop;
            if (location.isAtBottom) {
              // Reset cursor to last block when reaching bottom
              blockCursorRef.current =
                blockStartIndices.length > 0 ? blockStartIndices.length - 1 : 0;
            }
            // Only notify parent on edge transitions to avoid re-rendering on every scroll pixel
            if (atTop !== prevAtTop || location.isAtBottom !== prevAtBottom) {
              onScrollPositionChange?.({
                isAtTop: atTop,
                isAtBottom: location.isAtBottom,
              });
            }
          }}
          computeItemKey={computeItemKey}
          ItemContent={ItemContent}
        />
      </VirtuosoMessageListLicense>
    </div>
  );
});
