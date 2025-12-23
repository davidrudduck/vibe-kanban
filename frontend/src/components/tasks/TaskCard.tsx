import { useCallback, useEffect, useRef, useState } from 'react';
import { KanbanCard } from '@/components/ui/shadcn-io/kanban';
import {
  Archive,
  CheckCircle,
  Link,
  Loader2,
  Server,
  User,
  XCircle,
} from 'lucide-react';
import type { TaskWithAttemptStatus } from 'shared/types';
import { ActionsDropdown } from '@/components/ui/actions-dropdown';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { useNavigateWithSearch, useIsOrgAdmin } from '@/hooks';
import { useTaskLabels } from '@/hooks/useTaskLabels';
import { paths } from '@/lib/paths';
import type { SharedTaskRecord } from '@/hooks/useProjectTasks';
import { TaskCardHeader } from './TaskCardHeader';
import { useTranslation } from 'react-i18next';
import { useProject } from '@/contexts/ProjectContext';
import { cn } from '@/lib/utils';
import { LabelBadge } from '@/components/labels/LabelBadge';
import { ArchiveToggleIcon } from './ArchiveToggleIcon';
import { tasksApi } from '@/lib/api';

/**
 * Get short node name from full hostname (e.g., "justX" from "justX.raverx.net")
 */
function getShortNodeName(nodeName: string | null | undefined): string | null {
  if (!nodeName) return null;
  const dotIndex = nodeName.indexOf('.');
  return dotIndex > 0 ? nodeName.substring(0, dotIndex) : nodeName;
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

  // Fetch labels for this task (only for local tasks, not remote)
  const { data: labels } = useTaskLabels(
    task.is_remote ? undefined : task.id,
    !task.is_remote
  );

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
    if (isArchiving || task.is_remote) return;
    setIsArchiving(true);
    try {
      await tasksApi.archive(task.id, { include_subtasks: false });
    } catch (err) {
      console.error('Failed to archive task:', err);
    } finally {
      setIsArchiving(false);
    }
  }, [task.id, task.is_remote, isArchiving]);

  const handleUnarchive = useCallback(async () => {
    if (isArchiving || task.is_remote) return;
    setIsArchiving(true);
    try {
      await tasksApi.unarchive(task.id);
    } catch (err) {
      console.error('Failed to unarchive task:', err);
    } finally {
      setIsArchiving(false);
    }
  }, [task.id, task.is_remote, isArchiving]);

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
        task.is_remote
          ? 'relative overflow-hidden pl-5 before:absolute before:left-0 before:top-0 before:bottom-0 before:w-[3px] before:bg-purple-400 before:content-[""]'
          : sharedTask
            ? 'relative overflow-hidden pl-5 before:absolute before:left-0 before:top-0 before:bottom-0 before:w-[3px] before:bg-card-foreground before:content-[""]'
            : undefined,
        isArchived && 'opacity-60'
      )}
    >
      <div className="flex flex-col gap-2">
        <TaskCardHeader
          title={task.title}
          avatar={
            sharedTask
              ? {
                  firstName: sharedTask.assignee_first_name ?? undefined,
                  lastName: sharedTask.assignee_last_name ?? undefined,
                  username: sharedTask.assignee_username ?? undefined,
                }
              : task.is_remote && task.remote_assignee_name
                ? {
                    // Parse from remote_assignee_name (e.g., "John Doe")
                    firstName:
                      task.remote_assignee_name.split(' ')[0] ?? undefined,
                    lastName:
                      task.remote_assignee_name.split(' ').slice(1).join(' ') ||
                      undefined,
                    username: task.remote_assignee_username ?? undefined,
                  }
                : undefined
          }
          right={
            <>
              {isArchived && (
                <Badge
                  variant="secondary"
                  className="gap-1 px-1.5 py-0.5 text-xs"
                >
                  <Archive className="h-3 w-3" />
                  {t('badges.archived')}
                </Badge>
              )}
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
        {task.description && (
          <p className="text-sm text-secondary-foreground break-words">
            {task.description.length > 130
              ? `${task.description.substring(0, 130)}...`
              : task.description}
          </p>
        )}
        {/* Labels */}
        {labels && labels.length > 0 && (
          <div className="flex flex-wrap gap-1">
            {labels.map((label) => (
              <LabelBadge key={label.id} label={label} size="sm" />
            ))}
          </div>
        )}
        {/* Owner and node info */}
        {(ownerName || shortNodeName) && (
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
            {ownerName && (
              <div className="flex items-center gap-1">
                <User className="h-3 w-3" />
                <span className="truncate max-w-[100px]">{ownerName}</span>
              </div>
            )}
            {shortNodeName && (
              <div className="flex items-center gap-1">
                <Server className="h-3 w-3" />
                <span>{shortNodeName}</span>
              </div>
            )}
          </div>
        )}
        {/* Archive toggle icon */}
        <div className="flex justify-end mt-1">
          <ArchiveToggleIcon
            isArchived={isArchived}
            onArchive={handleArchive}
            onUnarchive={handleUnarchive}
            disabled={task.is_remote || isArchiving}
          />
        </div>
      </div>
    </KanbanCard>
  );
}
