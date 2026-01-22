import MarkdownRenderer from '@/components/ui/markdown-renderer.tsx';
import { cn } from '@/lib/utils';

interface ResultMessageCardProps {
  content: string;
  isError: boolean;
  subtype: string;
  durationMs: number;
  numTurns: number;
  totalCostUsd?: number;
}

export function ResultMessageCard({
  content,
  isError,
  durationMs,
  numTurns,
  totalCostUsd,
}: ResultMessageCardProps) {
  const borderColor = isError ? 'border-l-red-500' : 'border-l-green-500';
  const bgColor = isError
    ? 'bg-red-50 dark:bg-red-950/20'
    : 'bg-green-50 dark:bg-green-950/20';

  const formatDuration = (ms: number) => {
    const seconds = Math.floor(ms / 1000);
    const minutes = Math.floor(seconds / 60);
    const remainingSeconds = seconds % 60;
    return minutes > 0
      ? `${minutes}m ${remainingSeconds}s`
      : `${seconds}s`;
  };

  return (
    <div
      className={cn(
        'border-l-4 rounded-r-lg p-4',
        borderColor,
        bgColor
      )}
    >
      <div className="prose prose-sm dark:prose-invert max-w-none">
        <MarkdownRenderer content={content} />
      </div>
      <div className="mt-3 pt-3 border-t border-current/10 text-xs text-muted-foreground flex flex-wrap gap-x-4 gap-y-1">
        <span>Session {isError ? 'failed' : 'completed'}</span>
        <span>{formatDuration(durationMs)}</span>
        <span>{numTurns} turns</span>
        {totalCostUsd != null && (
          <span>${totalCostUsd.toFixed(2)} USD</span>
        )}
      </div>
    </div>
  );
}
