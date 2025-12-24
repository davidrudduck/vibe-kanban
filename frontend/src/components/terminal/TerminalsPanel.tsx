import { useEffect, useCallback, memo } from 'react';
import { X, Plus, Terminal, Loader2, AlertCircle } from 'lucide-react';
import { useAttemptWorktreePath } from '@/hooks/useTerminalSession';
import { useTerminalTabs, TerminalTab } from '@/hooks/useTerminalTabs';
import TerminalContainer from './TerminalContainer';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

interface TerminalsPanelProps {
  /** Task attempt ID for worktree terminal */
  attemptId?: string;
  /** Callback when user closes all terminals (mode should change) */
  onClose?: () => void;
  className?: string;
}

/** Individual tab button */
const TabButton = memo(function TabButton({
  tab,
  isActive,
  onSelect,
  onClose,
}: {
  tab: TerminalTab;
  isActive: boolean;
  onSelect: () => void;
  onClose: () => void;
}) {
  return (
    <div
      role="tab"
      aria-selected={isActive}
      tabIndex={isActive ? 0 : -1}
      onClick={onSelect}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onSelect();
        }
      }}
      className={cn(
        'group flex items-center gap-1.5 px-3 py-1.5 text-sm cursor-pointer',
        'border-b-2 transition-colors',
        isActive
          ? 'border-primary text-foreground bg-muted/50'
          : 'border-transparent text-muted-foreground hover:text-foreground hover:bg-muted/30'
      )}
    >
      <Terminal className="h-3.5 w-3.5 shrink-0" />
      <span className="truncate max-w-[120px]">{tab.label}</span>
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          onClose();
        }}
        className={cn(
          'ml-1 p-0.5 rounded-sm transition-opacity',
          'opacity-0 group-hover:opacity-100 focus:opacity-100',
          'hover:bg-muted-foreground/20'
        )}
        aria-label={`Close ${tab.label}`}
      >
        <X className="h-3 w-3" />
      </button>
    </div>
  );
});

/** Add new terminal button */
const AddTabButton = memo(function AddTabButton({
  onClick,
  disabled,
}: {
  onClick: () => void;
  disabled: boolean;
}) {
  return (
    <Button
      variant="ghost"
      size="sm"
      onClick={onClick}
      disabled={disabled}
      className="h-8 px-2"
      aria-label="Open new terminal"
    >
      <Plus className="h-4 w-4" />
    </Button>
  );
});

/**
 * Tabbed terminal panel for displaying multiple terminals in a task attempt's worktree.
 * Supports adding up to 5 terminals, switching between them, and closing individual tabs.
 */
export function TerminalsPanel({
  attemptId,
  onClose,
  className,
}: TerminalsPanelProps) {
  // Get the worktree path for this attempt
  const {
    data: worktreeData,
    isLoading,
    error,
  } = useAttemptWorktreePath(attemptId);

  const { tabs, activeTabId, addTab, removeTab, setActiveTab } =
    useTerminalTabs({ maxTabs: 5 });

  // Auto-create first terminal when worktree path is available
  useEffect(() => {
    if (worktreeData?.path && tabs.length === 0) {
      addTab(worktreeData.path, 'Terminal');
    }
  }, [worktreeData?.path, tabs.length, addTab]);

  const handleAddTab = useCallback(() => {
    if (worktreeData?.path) {
      addTab(worktreeData.path);
    }
  }, [worktreeData?.path, addTab]);

  const handleRemoveTab = useCallback(
    (tabId: string) => {
      removeTab(tabId);
      // If no tabs left, close the panel
      if (tabs.length === 1) {
        onClose?.();
      }
    },
    [removeTab, tabs.length, onClose]
  );

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

  const activeTab = tabs.find((t) => t.id === activeTabId);

  return (
    <div className={cn('flex flex-col h-full', className)}>
      {/* Tab bar */}
      <div className="flex items-center border-b bg-muted/30 shrink-0">
        <div
          role="tablist"
          aria-label="Terminal tabs"
          className="flex items-center overflow-x-auto flex-1 min-w-0"
        >
          {tabs.map((tab) => (
            <TabButton
              key={tab.id}
              tab={tab}
              isActive={tab.id === activeTabId}
              onSelect={() => setActiveTab(tab.id)}
              onClose={() => handleRemoveTab(tab.id)}
            />
          ))}
        </div>
        <div className="flex items-center px-2 shrink-0">
          <AddTabButton onClick={handleAddTab} disabled={tabs.length >= 5} />
        </div>
      </div>

      {/* Terminal content area */}
      <div className="flex-1 min-h-0 relative">
        {tabs.map((tab) => (
          <div
            key={tab.id}
            role="tabpanel"
            aria-labelledby={tab.id}
            hidden={tab.id !== activeTabId}
            className={cn(
              'absolute inset-0',
              tab.id !== activeTabId && 'invisible'
            )}
          >
            <TerminalContainer workingDir={tab.workingDir} className="h-full" />
          </div>
        ))}

        {/* Empty state when no tabs */}
        {tabs.length === 0 && activeTab === undefined && (
          <div className="flex flex-col items-center justify-center h-full text-muted-foreground">
            <Terminal className="h-8 w-8 mb-2 opacity-50" />
            <p className="text-sm">No terminals open</p>
            <Button
              variant="outline"
              size="sm"
              onClick={handleAddTab}
              className="mt-3"
            >
              <Plus className="h-4 w-4 mr-2" />
              Open Terminal
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}

export default TerminalsPanel;
