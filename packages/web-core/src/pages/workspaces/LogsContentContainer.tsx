import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { cn } from '@/shared/lib/utils';
import {
  type LogEntry,
  VirtualizedProcessLogs,
} from '@/shared/components/VirtualizedProcessLogs';
import type { LogViewerHandle } from '@/shared/components/VirtualizedProcessLogs';
import { useLogStream } from '@/shared/hooks/useLogStream';
import { useLogsPanel } from '@/shared/hooks/useLogsPanel';
import { TerminalPanelContainer } from '@/shared/components/TerminalPanelContainer';
import {
  ArrowsInSimpleIcon,
  ArrowLineUpIcon,
  ArrowUpIcon,
  ArrowDownIcon,
  ArrowLineDownIcon,
} from '@phosphor-icons/react';

export type LogsPanelContent =
  | { type: 'process'; processId: string }
  | {
      type: 'tool';
      toolName: string;
      content: string;
      command: string | undefined;
    }
  | { type: 'terminal' };

interface LogsContentContainerProps {
  className: string;
}

function NavButton({
  icon: Icon,
  label,
  onClick,
}: {
  icon: React.ComponentType<{ className?: string; weight?: string }>;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="pointer-events-auto flex items-center justify-center size-8 rounded-full bg-secondary/80 backdrop-blur-sm border border-secondary text-low hover:text-normal hover:bg-secondary shadow-md transition-all"
      aria-label={label}
      title={label}
    >
      <Icon className="size-icon-base" weight="bold" />
    </button>
  );
}

export function LogsContentContainer({ className }: LogsContentContainerProps) {
  const {
    logsPanelContent: content,
    logSearchQuery: searchQuery,
    logCurrentMatchIdx: currentMatchIndex,
    setLogMatchIndices: onMatchIndicesChange,
    collapseTerminal,
  } = useLogsPanel();
  const { t } = useTranslation('common');
  // Get logs for process content (only when type is 'process')
  const processId = content?.type === 'process' ? content.processId : '';
  const logViewerRef = useRef<LogViewerHandle>(null);
  const [isAtTop, setIsAtTop] = useState(true);
  const [isAtBottom, setIsAtBottom] = useState(false);

  const handleScrollPositionChange = useCallback(
    ({
      isAtTop: top,
      isAtBottom: bottom,
    }: {
      isAtTop: boolean;
      isAtBottom: boolean;
    }) => {
      setIsAtTop(top);
      setIsAtBottom(bottom);
    },
    [],
  );

  const { logs, error, blockStartIndices } = useLogStream(processId);

  // Get the current logs based on content type
  const currentLogs = useMemo(() => {
    if (content?.type === 'tool') {
      return content.content
        .split('\n')
        .map((line) => ({ type: 'STDOUT' as const, content: line }));
    }
    return logs;
  }, [content, logs]);

  // Compute which log indices match the search query (reversed for bottom-to-top)
  const matchIndices = useMemo(() => {
    if (!searchQuery.trim()) return [];
    const query = searchQuery.toLowerCase();
    const matches = currentLogs
      .map((log, idx) => (log.content.toLowerCase().includes(query) ? idx : -1))
      .filter((idx) => idx !== -1);
    // Reverse so newest matches (bottom) come first
    return matches.reverse();
  }, [currentLogs, searchQuery]);

  // Report match indices to parent
  useEffect(() => {
    onMatchIndicesChange?.(matchIndices);
  }, [matchIndices, onMatchIndicesChange]);

  // Empty state
  if (!content) {
    return (
      <div className="w-full h-full bg-secondary flex items-center justify-center text-low">
        <p className="text-sm">{t('logs.selectProcessToView')}</p>
      </div>
    );
  }

  // Tool content - render static content using VirtualizedProcessLogs
  if (content.type === 'tool') {
    const toolLogs: LogEntry[] = content.content
      .split('\n')
      .map((line) => ({ type: 'STDOUT' as const, content: line }));

    return (
      <div className={cn('h-full bg-secondary flex flex-col relative', className)}>
        <div className="px-4 py-2 border-b border-border text-sm font-medium text-normal shrink-0">
          {content.toolName}
        </div>
        {content.command && (
          <div className="px-4 py-2 font-mono text-xs text-low border-b border-border bg-tertiary shrink-0">
            $ {content.command}
          </div>
        )}
        <div className="flex-1 min-h-0">
          <VirtualizedProcessLogs
            ref={logViewerRef}
            logs={toolLogs}
            error={null}
            searchQuery={searchQuery}
            matchIndices={matchIndices}
            currentMatchIndex={currentMatchIndex}
            blockStartIndices={[]}
            onScrollPositionChange={handleScrollPositionChange}
          />
        </div>
        {/* Nav overlay */}
        <div className="absolute right-2 bottom-2 z-10 flex flex-col gap-1 pointer-events-none">
          {!isAtTop && (
            <>
              <NavButton
                icon={ArrowLineUpIcon}
                label="Go to top"
                onClick={() => logViewerRef.current?.scrollToTop()}
              />
              <NavButton
                icon={ArrowUpIcon}
                label="Previous section"
                onClick={() => logViewerRef.current?.scrollToPrevBlock()}
              />
            </>
          )}
          {!isAtBottom && (
            <>
              <NavButton
                icon={ArrowDownIcon}
                label="Next section"
                onClick={() => logViewerRef.current?.scrollToNextBlock()}
              />
              <NavButton
                icon={ArrowLineDownIcon}
                label="Go to bottom"
                onClick={() => logViewerRef.current?.scrollToBottom()}
              />
            </>
          )}
        </div>
      </div>
    );
  }

  // Terminal content - render terminal with collapse button
  if (content.type === 'terminal') {
    return (
      <div className={cn('h-full bg-secondary flex flex-col', className)}>
        <div className="px-4 py-1 flex items-center justify-between shrink-0 h-8">
          <span className="text-sm font-medium text-normal">
            {t('processes.terminal')}
          </span>
          <button
            type="button"
            onClick={collapseTerminal}
            className="text-low hover:text-normal transition-colors"
            title={t('actions.collapse')}
          >
            <ArrowsInSimpleIcon className="size-icon-sm" weight="bold" />
          </button>
        </div>
        <div className="flex-1 flex min-h-0 border-t border-border">
          <div className="flex-1 min-h-0 w-full">
            <TerminalPanelContainer />
          </div>
        </div>
      </div>
    );
  }

  // Process logs - render with VirtualizedProcessLogs
  return (
    <div className={cn('h-full bg-secondary relative', className)}>
      <VirtualizedProcessLogs
        ref={logViewerRef}
        key={processId}
        logs={logs}
        error={error}
        searchQuery={searchQuery}
        matchIndices={matchIndices}
        currentMatchIndex={currentMatchIndex}
        blockStartIndices={blockStartIndices}
        onScrollPositionChange={handleScrollPositionChange}
      />
      {/* Nav overlay */}
      <div className="absolute right-2 bottom-2 z-10 flex flex-col gap-1 pointer-events-none">
        {!isAtTop && (
          <>
            <NavButton
              icon={ArrowLineUpIcon}
              label="Go to top"
              onClick={() => logViewerRef.current?.scrollToTop()}
            />
            <NavButton
              icon={ArrowUpIcon}
              label="Previous section"
              onClick={() => logViewerRef.current?.scrollToPrevBlock()}
            />
          </>
        )}
        {!isAtBottom && (
          <>
            <NavButton
              icon={ArrowDownIcon}
              label="Next section"
              onClick={() => logViewerRef.current?.scrollToNextBlock()}
            />
            <NavButton
              icon={ArrowLineDownIcon}
              label="Go to bottom"
              onClick={() => logViewerRef.current?.scrollToBottom()}
            />
          </>
        )}
      </div>
    </div>
  );
}
