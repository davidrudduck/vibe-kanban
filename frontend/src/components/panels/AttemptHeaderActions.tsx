import { useTranslation } from 'react-i18next';
import { Eye, FileDiff, FolderTree, X } from 'lucide-react';
import { Button } from '../ui/button';
import { ToggleGroup, ToggleGroupItem } from '../ui/toggle-group';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '../ui/tooltip';
import type { LayoutMode } from '../layout/TasksLayout';
import type { TaskAttempt, TaskWithAttemptStatus } from 'shared/types';
import { ActionsDropdown } from '../ui/actions-dropdown';
import { useIsOrgAdmin, useRemoteConnectionStatus } from '@/hooks';
import type { SharedTaskRecord } from '@/hooks/useProjectTasks';
import { ConnectionStatusBadge } from '@/components/common/ConnectionStatusBadge';

interface AttemptHeaderActionsProps {
  onClose: () => void;
  mode?: LayoutMode;
  onModeChange?: (mode: LayoutMode) => void;
  task: TaskWithAttemptStatus;
  attempt?: TaskAttempt | null;
  sharedTask?: SharedTaskRecord;
  isMobile?: boolean;
}

export const AttemptHeaderActions = ({
  onClose,
  mode,
  onModeChange,
  task,
  attempt,
  sharedTask,
  isMobile,
}: AttemptHeaderActionsProps) => {
  const { t } = useTranslation('tasks');
  const isOrgAdmin = useIsOrgAdmin();
  const { status: connectionStatus } = useRemoteConnectionStatus(task, {
    enabled: Boolean(attempt),
  });

  // Only show connection badge for remote tasks (not local)
  const showConnectionBadge = task?.is_remote && connectionStatus !== 'local';

  return (
    <>
      {/* Connection status badge for remote tasks */}
      {showConnectionBadge && (
        <>
          <ConnectionStatusBadge status={connectionStatus} />
          <div className="h-4 w-px bg-border" />
        </>
      )}
      {!isMobile && typeof mode !== 'undefined' && onModeChange && (
        <TooltipProvider>
          <ToggleGroup
            type="single"
            value={mode ?? ''}
            onValueChange={(v) => {
              const newMode = (v as LayoutMode) || null;
              onModeChange(newMode);
            }}
            className="inline-flex gap-4"
            aria-label="Layout mode"
          >
            <Tooltip>
              <TooltipTrigger asChild>
                <ToggleGroupItem
                  value="preview"
                  aria-label="Preview"
                  active={mode === 'preview'}
                >
                  <Eye className="h-4 w-4" />
                </ToggleGroupItem>
              </TooltipTrigger>
              <TooltipContent side="bottom">
                {t('attemptHeaderActions.preview')}
              </TooltipContent>
            </Tooltip>

            <Tooltip>
              <TooltipTrigger asChild>
                <ToggleGroupItem
                  value="diffs"
                  aria-label="Diffs"
                  active={mode === 'diffs'}
                >
                  <FileDiff className="h-4 w-4" />
                </ToggleGroupItem>
              </TooltipTrigger>
              <TooltipContent side="bottom">
                {t('attemptHeaderActions.diffs')}
              </TooltipContent>
            </Tooltip>

            <Tooltip>
              <TooltipTrigger asChild>
                <ToggleGroupItem
                  value="files"
                  aria-label="Files"
                  active={mode === 'files'}
                >
                  <FolderTree className="h-4 w-4" />
                </ToggleGroupItem>
              </TooltipTrigger>
              <TooltipContent side="bottom">
                {t('attemptHeaderActions.files', { defaultValue: 'Files' })}
              </TooltipContent>
            </Tooltip>
          </ToggleGroup>
        </TooltipProvider>
      )}
      {!isMobile && typeof mode !== 'undefined' && onModeChange && (
        <div className="h-4 w-px bg-border" />
      )}
      <ActionsDropdown task={task} attempt={attempt} sharedTask={sharedTask} isOrgAdmin={isOrgAdmin} />
      <Button variant="icon" aria-label="Close" onClick={onClose}>
        <X size={16} />
      </Button>
    </>
  );
};
