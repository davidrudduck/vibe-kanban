import { cn } from '@/lib/utils';
import {
  getDaysInColumn,
  formatDaysInColumn,
  getDaysStyle,
} from '@/utils/daysInColumn';

interface DaysInColumnBadgeProps {
  /** The activity timestamp for the task (when it entered current status) */
  activityAt: Date | string | null | undefined;
  /** Additional CSS classes */
  className?: string;
}

/**
 * Badge component showing how many days a task has been in its current column.
 * Returns null if less than 1 day old.
 * Shows age-appropriate styling:
 * - 1-2 days: neutral/subtle
 * - 3-6 days: amber warning
 * - 7+ days: red strong warning
 */
export function DaysInColumnBadge({
  activityAt,
  className,
}: DaysInColumnBadgeProps) {
  const days = getDaysInColumn(activityAt);
  const formatted = formatDaysInColumn(days);

  // Don't render if less than 1 day
  if (!formatted) return null;

  const styleClasses = getDaysStyle(days);

  return (
    <span
      className={cn(
        'inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium',
        styleClasses,
        className
      )}
      title={`${days} day${days === 1 ? '' : 's'} in this column`}
    >
      {formatted}
    </span>
  );
}
