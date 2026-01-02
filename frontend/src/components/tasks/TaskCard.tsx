import { useCallback, useEffect, useRef, useState } from 'react';
import { KanbanCard } from '@/components/ui/shadcn-io/kanban';
import { CheckCircle, Link, Loader2, XCircle } from 'lucide-react';
import type { TaskStatus, TaskWithAttemptStatus } from 'shared/types';
import { ActionsDropdown } from '@/components/ui/actions-dropdown';
import { Button } from '@/components/ui/button';
import { useNavigateWithSearch, useIsOrgAdmin } from '@/hooks';
import { useTaskLabels } from '@/hooks/useTaskLabels';
import { paths } from '@/lib/paths';
import type { SharedTaskRecord } from '@/hooks/useProjectTasks';
import { TaskCardHeader } from './TaskCardHeader';
import { useTranslation } from 'react-i18next';
import { useProject } from '@/contexts/ProjectContext';
import { cn } from '@/lib/utils';
import { ArchiveToggleIcon } from './ArchiveToggleIcon';
import { CompactLabelList } from './CompactLabelList';
import { DaysInColumnBadge } from './DaysInColumnBadge';
import { tasksApi } from '@/lib/api';
import {
  useTaskOptimistic,
  getArchivedCallback,
} from '@/contexts/TaskOptimisticContext';

/**
 * Status color mapping for the left strip indicator.
 * Uses CSS custom properties for theme consistency.
 */
const statusStripColors: Record<TaskStatus, string> = {
  todo: 'before:bg-neutral-400 dark:before:bg-neutral-500',
  inprogress: 'before:bg-blue-500',
  inreview: 'before:bg-amber-500',
  done: 'before:bg-green-500',
  cancelled: 'before:bg-red-500',
};

/**
 * Get short node name from full hostname (e.g., "justX" from "justX.raverx.net")
 */
function getShortNodeName(nodeName: string | null | undefined): string | null {
  if (!nodeName) return null;
  const dotIndex = nodeName.indexOf('.');
  return dotIndex > 0 ? nodeName.substring(0, dotIndex) : nodeName;
}

/**
 * Truncate description to a maximum length, adding ellipsis if needed.
 */
function truncateDescription(
  description: string | null | undefined,
  maxLength: number = 40
): string | null {
  if (!description) return null;
  if (description.length <= maxLength) return description;
  return `${description.substring(0, maxLength)}...`;
}

type Task = TaskWithAttemptStatus;

interface TaskCardProps {
  task: Task;
  index: number;
  status: string;
  onViewDetails: (task: Task) => void;
  isOpen?: boolean;
  projectId: string;
  sharedTask?: SharedTaskRecord;
}

export function TaskCard({
  task,
  index,
  status,
  onViewDetails,
  isOpen,
  projectId,
  sharedTask,
}: TaskCardProps) {
  const { t } = useTranslation('tasks');
  const navigate = useNavigateWithSearch();
  const isOrgAdmin = useIsOrgAdmin();
  const { project } = useProject();
  const taskOptimisticContext = useTaskOptimistic();

  // Get optimistic archived callback from context or global registry
  const updateTaskArchivedOptimistically =
    taskOptimisticContext?.updateTaskArchivedOptimistically ??
    getArchivedCallback(projectId);

  // Fetch labels for this task
  const { data: labels } = useTaskLabels(task.id, true);

  // Get owner name from shared task or remote task
  const ownerName =
    sharedTask?.assignee_first_name || sharedTask?.assignee_last_name
      ? [sharedTask.assignee_first_name, sharedTask.assignee_last_name]
          .filter(Boolean)
          .join(' ')
      : (task.remote_assignee_name ?? task.remote_assignee_username ?? null);

  // Get short node name from project
  const shortNodeName = getShortNodeName(project?.source_node_name);

  const handleClick = useCallback(() => {
    onViewDetails(task);
  }, [task, onViewDetails]);

  const handleParentClick = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      if (!task.parent_task_id) return;

      // Direct navigation to parent task
      navigate(paths.task(projectId, task.parent_task_id));
    },
    [task.parent_task_id, projectId, navigate]
  );

  const localRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!isOpen || !localRef.current) return;
    const el = localRef.current;
    requestAnimationFrame(() => {
      el.scrollIntoView({
        block: 'center',
        inline: 'nearest',
        behavior: 'smooth',
      });
    });
  }, [isOpen]);

  const isArchived = task.archived_at !== null;
  const [isArchiving, setIsArchiving] = useState(false);

  const handleArchive = useCallback(async () => {
    if (isArchiving) return;
    setIsArchiving(true);
    try {
      // Apply optimistic update immediately for instant UI feedback
      if (updateTaskArchivedOptimistically) {
        updateTaskArchivedOptimistically(task.id, new Date().toISOString());
      }
      await tasksApi.archive(task.id, { include_subtasks: false });
    } catch (err) {
      console.error('Failed to archive task:', err);
      // Rollback optimistic update on error
      if (updateTaskArchivedOptimistically) {
        updateTaskArchivedOptimistically(task.id, null);
      }
    } finally {
      setIsArchiving(false);
    }
  }, [task.id, isArchiving, updateTaskArchivedOptimistically]);

  const handleUnarchive = useCallback(async () => {
    if (isArchiving) return;
    setIsArchiving(true);
    const previousArchivedAt = task.archived_at;
    try {
      // Apply optimistic update immediately for instant UI feedback
      if (updateTaskArchivedOptimistically) {
        updateTaskArchivedOptimistically(task.id, null);
      }
      await tasksApi.unarchive(task.id);
    } catch (err) {
      console.error('Failed to unarchive task:', err);
      // Rollback optimistic update on error
      if (updateTaskArchivedOptimistically && previousArchivedAt) {
        updateTaskArchivedOptimistically(
          task.id,
          typeof previousArchivedAt === 'string'
            ? previousArchivedAt
            : previousArchivedAt.toISOString()
        );
      }
    } finally {
      setIsArchiving(false);
    }
  }, [
    task.id,
    task.archived_at,
    isArchiving,
    updateTaskArchivedOptimistically,
  ]);

  // Get status strip color - shared tasks use their own color, otherwise use status
  const statusStripClass = sharedTask
    ? 'before:bg-card-foreground'
    : statusStripColors[task.status as TaskStatus] || statusStripColors['todo'];

  // Truncated description for compact view
  const truncatedDesc = truncateDescription(task.description, 40);

  return (
    <KanbanCard
      key={task.id}
      id={task.id}
      name={task.title}
      index={index}
      parent={status}
      onClick={handleClick}
      isOpen={isOpen}
      forwardedRef={localRef}
      className={cn(
        // Status strip indicator (left border)
        'relative overflow-hidden pl-5',
        'before:absolute before:left-0 before:top-0 before:bottom-0 before:w-[3px] before:content-[""]',
        statusStripClass,
        isArchived && 'opacity-60'
      )}
    >
      <div className="flex flex-col gap-1.5">
        <TaskCardHeader
          title={task.title}
          avatar={
            sharedTask
              ? {
                  firstName: sharedTask.assignee_first_name ?? undefined,
                  lastName: sharedTask.assignee_last_name ?? undefined,
                  username: sharedTask.assignee_username ?? undefined,
                  ownerName,
                  nodeName: shortNodeName,
                }
              : task.remote_assignee_name
                ? {
                    firstName:
                      task.remote_assignee_name.split(' ')[0] ?? undefined,
                    lastName:
                      task.remote_assignee_name.split(' ').slice(1).join(' ') ||
                      undefined,
                    username: task.remote_assignee_username ?? undefined,
                    ownerName,
                    nodeName: shortNodeName,
                  }
                : ownerName || shortNodeName
                  ? {
                      ownerName,
                      nodeName: shortNodeName,
                    }
                  : undefined
          }
          right={
            <>
              {task.has_in_progress_attempt && (
                <Loader2 className="h-4 w-4 animate-spin text-blue-500" />
              )}
              {task.has_merged_attempt && (
                <CheckCircle className="h-4 w-4 text-green-500" />
              )}
              {task.last_attempt_failed && !task.has_merged_attempt && (
                <XCircle className="h-4 w-4 text-destructive" />
              )}
              {task.parent_task_id && (
                <Button
                  variant="icon"
                  onClick={handleParentClick}
                  onPointerDown={(e) => e.stopPropagation()}
                  onMouseDown={(e) => e.stopPropagation()}
                  title={t('navigateToParent')}
                >
                  <Link className="h-4 w-4" />
                </Button>
              )}
              <ActionsDropdown
                task={task}
                sharedTask={sharedTask}
                isOrgAdmin={isOrgAdmin}
              />
            </>
          }
        />
        {/* Truncated description - single line */}
        {truncatedDesc && (
          <p
            className="text-xs text-muted-foreground truncate"
            title={task.description ?? undefined}
          >
            {truncatedDesc}
          </p>
        )}
        {/* Compact footer: Node name, Labels, Days badge, Archive */}
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-2 min-w-0 overflow-hidden">
            {shortNodeName && (
              <span className="text-xs text-muted-foreground shrink-0">
                {shortNodeName}
              </span>
            )}
            <CompactLabelList labels={labels} maxVisible={2} size="sm" />
          </div>
          <div className="flex items-center gap-1.5 shrink-0">
            <DaysInColumnBadge activityAt={task.activity_at} />
            <ArchiveToggleIcon
              isArchived={isArchived}
              onArchive={handleArchive}
              onUnarchive={handleUnarchive}
              disabled={isArchiving}
            />
          </div>
        </div>
      </div>
    </KanbanCard>
  );
}
