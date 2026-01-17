import React from 'react';
import { cn } from '@/lib/utils';
import { useExpandable } from '@/stores/useExpandableStore';
import MarkdownRenderer from '@/components/ui/markdown-renderer.tsx';
import { ChevronDown } from 'lucide-react';

type ResultAppearance = 'success' | 'error' | 'warning';

type CollapsibleVariant = 'system' | 'error';

const ExpandChevron: React.FC<{
  expanded: boolean;
  onClick: () => void;
  variant: CollapsibleVariant;
}> = ({ expanded, onClick, variant }) => {
  const color =
    variant === 'system'
      ? 'text-700 dark:text-300'
      : 'text-red-700 dark:text-red-300';

  return (
    <ChevronDown
      onClick={onClick}
      className={`h-4 w-4 cursor-pointer transition-transform ${color} ${
        expanded ? '' : '-rotate-90'
      }`}
    />
  );
};

const RESULT_APPEARANCE: Record<
  ResultAppearance,
  {
    border: string;
    headerBg: string;
    headerText: string;
    contentBg: string;
    contentText: string;
  }
> = {
  success: {
    border: 'border-green-400/40',
    headerBg: 'bg-green-50 dark:bg-green-950/20',
    headerText: 'text-green-700 dark:text-green-300',
    contentBg: 'bg-green-50 dark:bg-green-950/20',
    contentText: 'text-green-700 dark:text-green-300',
  },
  error: {
    border: 'border-red-400/40',
    headerBg: 'bg-red-50 dark:bg-red-950/20',
    headerText: 'text-red-700 dark:text-red-300',
    contentBg: 'bg-red-50 dark:bg-red-950/10',
    contentText: 'text-red-700 dark:text-red-300',
  },
  warning: {
    border: 'border-amber-400/40',
    headerBg: 'bg-amber-50 dark:bg-amber-950/20',
    headerText: 'text-amber-700 dark:text-amber-200',
    contentBg: 'bg-amber-50 dark:bg-amber-950/10',
    contentText: 'text-amber-700 dark:text-amber-200',
  },
};

function getAppearanceFromSubtype(
  subtype: string,
  isError: boolean
): ResultAppearance {
  if (isError) {
    // Warning-level errors (budget/retry limits)
    if (
      subtype === 'error_max_budget_usd' ||
      subtype === 'error_max_structured_output_retries'
    ) {
      return 'warning';
    }
    // Error-level failures (max turns, execution errors)
    return 'error';
  }
  // Success
  return 'success';
}

function formatDuration(durationMs: number): string {
  if (durationMs < 1000) {
    return `${durationMs}ms`;
  }
  const seconds = Math.floor(durationMs / 1000);
  if (seconds < 60) {
    return `${seconds}s`;
  }
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  if (minutes < 60) {
    return remainingSeconds > 0
      ? `${minutes}m ${remainingSeconds}s`
      : `${minutes}m`;
  }
  const hours = Math.floor(minutes / 60);
  const remainingMinutes = minutes % 60;
  return remainingMinutes > 0 ? `${hours}h ${remainingMinutes}m` : `${hours}h`;
}

type Props = {
  subtype: string;
  content: string;
  durationMs: number;
  numTurns: number;
  isError: boolean;
  expansionKey: string;
  defaultExpanded?: boolean;
};

const ResultMessageCard: React.FC<Props> = ({
  subtype,
  content,
  durationMs,
  numTurns,
  isError,
  expansionKey,
  defaultExpanded = false,
}) => {
  const [expanded, toggle] = useExpandable(
    `result-entry:${expansionKey}`,
    defaultExpanded
  );

  const appearance = getAppearanceFromSubtype(subtype, isError);
  const tone = RESULT_APPEARANCE[appearance];
  const duration = formatDuration(durationMs);

  return (
    <div className="inline-block w-full">
      <div
        className={cn('border w-full overflow-hidden rounded-sm', tone.border)}
      >
        <button
          onClick={(e: React.MouseEvent) => {
            e.preventDefault();
            toggle();
          }}
          title={expanded ? 'Hide session result' : 'Show session result'}
          className={cn(
            'w-full px-2 py-1.5 flex items-center gap-1.5 text-left border-b',
            tone.headerBg,
            tone.headerText,
            tone.border
          )}
        >
          <span className="min-w-0 truncate">
            <span className="font-semibold">Session Result</span>
            <span className="ml-2 text-xs opacity-80">
              {numTurns} {numTurns === 1 ? 'turn' : 'turns'} • {duration}
            </span>
          </span>
          <div className="ml-auto flex items-center gap-2">
            <ExpandChevron
              expanded={expanded}
              onClick={toggle}
              variant={appearance === 'error' ? 'error' : 'system'}
            />
          </div>
        </button>

        {expanded && (
          <div className={cn('px-3 py-2', tone.contentBg)}>
            <div className={cn('text-sm', tone.contentText)}>
              <MarkdownRenderer
                content={content}
                className="whitespace-pre-wrap break-words"
                enableCopyButton
              />
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

export default ResultMessageCard;
