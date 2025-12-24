import { Loader2, AlertCircle, Terminal } from 'lucide-react';
import { useAttemptWorktreePath } from '@/hooks/useTerminalSession';
import TerminalContainer from './TerminalContainer';
import { cn } from '@/lib/utils';

interface TerminalPanelProps {
  /** Task attempt ID for worktree terminal */
  attemptId?: string;
  /** Callback when user closes the terminal */
  onClose?: () => void;
  className?: string;
}

/**
 * Terminal panel for displaying a terminal in a task attempt's worktree.
 * Fetches the worktree path and passes it to TerminalContainer.
 */
export function TerminalPanel({
  attemptId,
  onClose,
  className,
}: TerminalPanelProps) {
  // Get the worktree path for this attempt
  const {
    data: worktreeData,
    isLoading,
    error,
  } = useAttemptWorktreePath(attemptId);

  // No attempt ID provided
  if (!attemptId) {
    return (
      <div
        className={cn(
          'flex flex-col items-center justify-center h-full text-muted-foreground p-4',
          className
        )}
      >
        <Terminal className="h-8 w-8 mb-2 opacity-50" />
        <p className="text-sm text-center">
          No task attempt selected. Select an active task to open a terminal.
        </p>
      </div>
    );
  }

  // Loading state
  if (isLoading) {
    return (
      <div
        className={cn(
          'flex flex-col items-center justify-center h-full text-muted-foreground',
          className
        )}
      >
        <Loader2 className="h-8 w-8 animate-spin mb-2" />
        <span className="text-sm">Loading worktree path...</span>
      </div>
    );
  }

  // Error state
  if (error || !worktreeData?.path) {
    return (
      <div
        className={cn(
          'flex flex-col items-center justify-center h-full text-muted-foreground p-4',
          className
        )}
      >
        <AlertCircle className="h-8 w-8 text-destructive mb-2" />
        <p className="text-sm text-center">
          {error instanceof Error
            ? error.message
            : 'Failed to get worktree path'}
        </p>
      </div>
    );
  }

  // Render the terminal
  return (
    <TerminalContainer
      workingDir={worktreeData.path}
      className={cn('h-full', className)}
      onClose={onClose}
    />
  );
}

export default TerminalPanel;
