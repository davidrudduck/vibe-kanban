import { useState, useCallback, useMemo } from 'react';
import { useSwipe } from '@/hooks/useSwipe';
import { cn } from '@/lib/utils';
import type { TaskStatus, TaskWithProjectInfo } from 'shared/types';
import { statusBoardColors, statusLabels } from '@/utils/statusLabels';
import MobileColumnHeader from './MobileColumnHeader';
import { AllProjectsTaskCard } from './AllProjectsTaskCard';

/**
 * Ordered list of task statuses for navigation
 */
const COLUMN_ORDER: TaskStatus[] = [
  'todo',
  'inprogress',
  'inreview',
  'done',
  'cancelled',
];

export type AllProjectsKanbanColumns = Record<
  TaskStatus,
  TaskWithProjectInfo[]
>;

interface MobileAllProjectsKanbanProps {
  columns: AllProjectsKanbanColumns;
  onViewTaskDetails: (task: TaskWithProjectInfo) => void;
  className?: string;
}

/**
 * Mobile-optimized Kanban board for all-projects view that shows one column at a time with swipe navigation.
 * Uses the useSwipe hook for gesture detection and MobileColumnHeader for navigation.
 */
function MobileAllProjectsKanban({
  columns,
  onViewTaskDetails,
  className,
}: MobileAllProjectsKanbanProps) {
  const [currentColumnIndex, setCurrentColumnIndex] = useState(0);

  const currentStatus = COLUMN_ORDER[currentColumnIndex];

  const goToPrevColumn = useCallback(() => {
    setCurrentColumnIndex((prev) => Math.max(0, prev - 1));
  }, []);

  const goToNextColumn = useCallback(() => {
    setCurrentColumnIndex((prev) =>
      Math.min(COLUMN_ORDER.length - 1, prev + 1)
    );
  }, []);

  // Swipe handlers - swipe left goes to next, swipe right goes to previous
  const swipeHandlers = useSwipe(
    {
      onSwipeLeft: goToNextColumn,
      onSwipeRight: goToPrevColumn,
    },
    { threshold: 50, maxTime: 300 }
  );

  // Memoize column counts for header display
  const columnCounts = useMemo(() => {
    return COLUMN_ORDER.reduce(
      (acc, status) => {
        acc[status] = columns[status]?.length || 0;
        return acc;
      },
      {} as Record<TaskStatus, number>
    );
  }, [columns]);

  return (
    <div
      className={cn('flex flex-col h-full', className)}
      data-testid="mobile-all-projects-kanban"
    >
      <MobileColumnHeader
        name={statusLabels[currentStatus]}
        count={columnCounts[currentStatus]}
        color={statusBoardColors[currentStatus]}
        isFirst={currentColumnIndex === 0}
        isLast={currentColumnIndex === COLUMN_ORDER.length - 1}
        onPrev={goToPrevColumn}
        onNext={goToNextColumn}
        currentIndex={currentColumnIndex}
        totalColumns={COLUMN_ORDER.length}
      />

      <div
        className="flex-1 overflow-y-auto overflow-x-hidden"
        {...swipeHandlers}
        data-testid="swipeable-area"
      >
        <div
          className="flex transition-transform duration-250 ease-out"
          style={{
            width: `${COLUMN_ORDER.length * 100}%`,
            transform: `translateX(-${(currentColumnIndex * 100) / COLUMN_ORDER.length}%)`,
          }}
        >
          {COLUMN_ORDER.map((status, colIndex) => {
            const items = columns[status] || [];
            return (
              <div
                key={status}
                className="flex-shrink-0 flex flex-col gap-0"
                style={{ width: `${100 / COLUMN_ORDER.length}%` }}
                data-testid={`column-${status}`}
                aria-hidden={colIndex !== currentColumnIndex}
              >
                {items.map((task, index) => (
                  <AllProjectsTaskCard
                    key={task.id}
                    task={task}
                    index={index}
                    status={status}
                    onViewDetails={onViewTaskDetails}
                  />
                ))}
                {items.length === 0 && (
                  <div
                    className="flex items-center justify-center h-32 text-muted-foreground text-sm"
                    data-testid="empty-column"
                  >
                    No tasks
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

export default MobileAllProjectsKanban;
