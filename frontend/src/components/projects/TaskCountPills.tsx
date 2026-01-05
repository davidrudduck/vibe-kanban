import { cn } from '@/lib/utils';
import type { TaskCounts } from 'shared/types';
import { useNavigateWithSearch } from '@/hooks';

type Props = {
  counts: TaskCounts;
  projectId: string;
  /** Make pills more compact for mobile */
  compact?: boolean;
};

type StatusConfig = {
  key: 'todo' | 'inprogress' | 'inreview' | 'done';
  label: string;
  compactLabel: string;
  count: number;
  colorClass: string;
  bgClass: string;
};

/**
 * Clickable task status count pills for quick navigation to filtered kanban view.
 * Nordic Clean aesthetic - subtle, muted colors with refined typography.
 */
export function TaskCountPills({ counts, projectId, compact = false }: Props) {
  const navigate = useNavigateWithSearch();

  const statuses: StatusConfig[] = [
    {
      key: 'todo',
      label: 'Todo',
      compactLabel: 'T',
      count: counts.todo,
      colorClass: 'text-muted-foreground',
      bgClass: 'bg-muted/50 hover:bg-muted',
    },
    {
      key: 'inprogress',
      label: 'In Progress',
      compactLabel: 'WIP',
      count: counts.in_progress,
      colorClass: 'text-amber-600 dark:text-amber-400',
      bgClass: 'bg-amber-50/50 hover:bg-amber-50 dark:bg-amber-900/20 dark:hover:bg-amber-900/30',
    },
    {
      key: 'inreview',
      label: 'Review',
      compactLabel: 'Rev',
      count: counts.in_review,
      colorClass: 'text-blue-600 dark:text-blue-400',
      bgClass: 'bg-blue-50/50 hover:bg-blue-50 dark:bg-blue-900/20 dark:hover:bg-blue-900/30',
    },
    {
      key: 'done',
      label: 'Done',
      compactLabel: 'D',
      count: counts.done,
      colorClass: 'text-emerald-600 dark:text-emerald-400',
      bgClass: 'bg-emerald-50/50 hover:bg-emerald-50 dark:bg-emerald-900/20 dark:hover:bg-emerald-900/30',
    },
  ];

  const handleClick = (statusKey: string, e: React.MouseEvent) => {
    e.stopPropagation();
    navigate(`/projects/${projectId}/tasks?status=${statusKey}`);
  };

  return (
    <div className="flex gap-1.5 sm:gap-2">
      {statuses.map(({ key, label, compactLabel, count, colorClass, bgClass }) => (
        <button
          key={key}
          onClick={(e) => handleClick(key, e)}
          className={cn(
            'flex flex-col items-center rounded-lg transition-colors',
            'focus:outline-none focus:ring-2 focus:ring-primary/50',
            bgClass,
            compact ? 'px-2 py-1 min-w-[36px]' : 'px-3 py-1.5 min-w-[48px]'
          )}
          title={`${count} ${label}`}
        >
          <span
            className={cn(
              'font-semibold tabular-nums',
              colorClass,
              compact ? 'text-sm' : 'text-lg'
            )}
          >
            {count}
          </span>
          <span
            className={cn(
              'text-muted-foreground font-medium',
              compact ? 'text-[9px]' : 'text-[10px]'
            )}
          >
            {compact ? compactLabel : label}
          </span>
        </button>
      ))}
    </div>
  );
}

export default TaskCountPills;
