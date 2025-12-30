import { useCallback, useEffect, useRef } from 'react';
import { KanbanCard } from '@/components/ui/shadcn-io/kanban';
import type { TaskStatus } from 'shared/types';
import type { SharedTaskRecord } from '@/hooks/useProjectTasks';
import { TaskCardHeader } from './TaskCardHeader';
import { ActionsDropdown } from '@/components/ui/actions-dropdown';
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

interface SharedTaskCardProps {
  task: SharedTaskRecord;
  index: number;
  status: string;
  onViewDetails?: (task: SharedTaskRecord) => void;
  isSelected?: boolean;
  isOrgAdmin?: boolean;
}

export function SharedTaskCard({
  task,
  index,
  status,
  onViewDetails,
  isSelected,
  isOrgAdmin = false,
}: SharedTaskCardProps) {
  const localRef = useRef<HTMLDivElement>(null);

  const handleClick = useCallback(() => {
    onViewDetails?.(task);
  }, [onViewDetails, task]);

  useEffect(() => {
    if (!isSelected || !localRef.current) return;
    const el = localRef.current;
    requestAnimationFrame(() => {
      el.scrollIntoView({
        block: 'center',
        inline: 'nearest',
        behavior: 'smooth',
      });
    });
  }, [isSelected]);

  // Get owner name for tooltip
  const ownerName =
    task.assignee_first_name || task.assignee_last_name
      ? [task.assignee_first_name, task.assignee_last_name]
          .filter(Boolean)
          .join(' ')
      : null;

  // Get status strip color
  const statusStripClass =
    statusStripColors[task.status as TaskStatus] || statusStripColors['todo'];

  // Truncated description for compact view
  const truncatedDesc = truncateDescription(task.description, 40);

  return (
    <KanbanCard
      id={`shared-${task.id}`}
      name={task.title}
      index={index}
      parent={status}
      onClick={handleClick}
      isOpen={isSelected}
      forwardedRef={localRef}
      dragDisabled
      className={cn(
        'relative overflow-hidden pl-5',
        'before:absolute before:left-0 before:top-0 before:bottom-0 before:w-[3px] before:content-[""]',
        statusStripClass
      )}
    >
      <div className="flex flex-col gap-1.5">
        <TaskCardHeader
          title={task.title}
          avatar={{
            firstName: task.assignee_first_name ?? undefined,
            lastName: task.assignee_last_name ?? undefined,
            username: task.assignee_username ?? undefined,
            ownerName,
          }}
          right={
            isOrgAdmin ? (
              <ActionsDropdown sharedTask={task} isOrgAdmin={isOrgAdmin} />
            ) : undefined
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
        {/* Compact footer with days badge */}
        <div className="flex items-center justify-end">
          <DaysInColumnBadge activityAt={task.activity_at} />
        </div>
      </div>
    </KanbanCard>
  );
}
