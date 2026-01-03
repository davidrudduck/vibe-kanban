import { useCallback, useRef } from 'react';
import { KanbanCard } from '@/components/ui/shadcn-io/kanban';
import { CheckCircle, FolderOpen, Loader2, XCircle } from 'lucide-react';
import type { TaskStatus, TaskWithProjectInfo } from 'shared/types';
import { TaskCardHeader } from './TaskCardHeader';
import { DaysInColumnBadge } from './DaysInColumnBadge';
import { cn } from '@/lib/utils';

/**
 * Status color mapping for the left strip indicator.
 */
const statusStripColors: Record<TaskStatus, string> = {
  todo: 'before:bg-neutral-400 dark:before:bg-neutral-500',
  inprogress: 'before:bg-blue-500',
  inreview: 'before:bg-amber-500',
  done: 'before:bg-green-500',
  cancelled: 'before:bg-red-500',
};

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

/**
 * Get short node name from full hostname (e.g., "justX" from "justX.raverx.net")
 */
function getShortNodeName(nodeName: string | null | undefined): string | null {
  if (!nodeName) return null;
  const dotIndex = nodeName.indexOf('.');
  return dotIndex > 0 ? nodeName.substring(0, dotIndex) : nodeName;
}

interface AllProjectsTaskCardProps {
  task: TaskWithProjectInfo;
  index: number;
  status: string;
  onViewDetails: (task: TaskWithProjectInfo) => void;
}

export function AllProjectsTaskCard({
  task,
  index,
  status,
  onViewDetails,
}: AllProjectsTaskCardProps) {
  const localRef = useRef<HTMLDivElement>(null);

  const handleClick = useCallback(() => {
    onViewDetails(task);
  }, [task, onViewDetails]);

  const shortNodeName = getShortNodeName(task.source_node_name);

  // Prefer granular assignee fields if available, fall back to remote_assignee_name
  const ownerName =
    task.assignee_first_name || task.assignee_last_name
      ? [task.assignee_first_name, task.assignee_last_name]
          .filter(Boolean)
          .join(' ')
      : (task.remote_assignee_name ?? task.remote_assignee_username ?? null);

  // Get status strip color based on task status
  const statusStripClass =
    statusStripColors[task.status as TaskStatus] || statusStripColors['todo'];

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
      forwardedRef={localRef}
      className={cn(
        'relative overflow-hidden pl-5',
        'before:absolute before:left-0 before:top-0 before:bottom-0 before:w-[3px] before:content-[""]',
        statusStripClass
      )}
    >
      <div className="flex flex-col gap-1.5">
        <TaskCardHeader
          title={task.title}
          avatar={
            // Prefer granular assignee fields if available
            task.assignee_first_name ||
            task.assignee_last_name ||
            task.assignee_username
              ? {
                  firstName: task.assignee_first_name ?? undefined,
                  lastName: task.assignee_last_name ?? undefined,
                  username: task.assignee_username ?? undefined,
                  ownerName,
                  nodeName: shortNodeName,
                }
              : // Fallback: parse combined name for legacy data
                task.remote_assignee_name
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
        {/* Compact footer: Project name, node name, and days badge */}
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-1 text-xs text-muted-foreground min-w-0">
            <FolderOpen className="h-3 w-3 shrink-0" />
            <span className="truncate">{task.project_name}</span>
            {shortNodeName && (
              <>
                <span className="shrink-0">·</span>
                <span className="shrink-0">{shortNodeName}</span>
              </>
            )}
          </div>
          <DaysInColumnBadge activityAt={task.activity_at} />
        </div>
      </div>
    </KanbanCard>
  );
}
