import { useState, useCallback, useMemo } from 'react';
import { useSwipe } from '@/hooks/useSwipe';
import { cn } from '@/lib/utils';
import type { TaskStatus, TaskWithAttemptStatus } from 'shared/types';
import { statusBoardColors, statusLabels } from '@/utils/statusLabels';
import type { KanbanColumns } from './TaskKanbanBoard';
import type { SharedTaskRecord } from '@/hooks/useProjectTasks';
import MobileColumnHeader from './MobileColumnHeader';
import { TaskCard } from './TaskCard';
import { SharedTaskCard } from './SharedTaskCard';
import { SwipeableTaskCard } from './SwipeableTaskCard';
import { useAuth, useIsOrgAdmin } from '@/hooks';
import { tasksApi } from '@/lib/api';
import {
  useTaskOptimistic,
  getArchivedCallback,
} from '@/contexts/TaskOptimisticContext';

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

interface MobileKanbanBoardProps {
  columns: KanbanColumns;
  onViewTaskDetails: (task: TaskWithAttemptStatus) => void;
  onViewSharedTask?: (task: SharedTaskRecord) => void;
  selectedTaskId?: string;
  selectedSharedTaskId?: string | null;
  projectId: string;
  className?: string;
}

/**
 * Mobile-optimized Kanban board that shows one column at a time with swipe navigation.
 * Uses the useSwipe hook for gesture detection and MobileColumnHeader for navigation.
 */
function MobileKanbanBoard({
  columns,
  onViewTaskDetails,
  onViewSharedTask,
  selectedTaskId,
  selectedSharedTaskId,
  projectId,
  className,
}: MobileKanbanBoardProps) {
  const [currentColumnIndex, setCurrentColumnIndex] = useState(0);
  const { userId } = useAuth();
  const isOrgAdmin = useIsOrgAdmin();
  const taskOptimisticContext = useTaskOptimistic();

  // Get optimistic archived callback from context or global registry
  const updateTaskArchivedOptimistically =
    taskOptimisticContext?.updateTaskArchivedOptimistically ??
    getArchivedCallback(projectId);

  const currentStatus = COLUMN_ORDER[currentColumnIndex];

  // Handle swipe-to-archive for a task
  const handleSwipeArchive = useCallback(
    async (task: TaskWithAttemptStatus) => {
      if (task.is_remote || task.archived_at) return;

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
      }
    },
    [updateTaskArchivedOptimistically]
  );

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
      data-testid="mobile-kanban-board"
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
                {items.map((item, index) => {
                  // Admins can manage all tasks, so they see TaskCard for everything
                  const isOwnTask =
                    item.type === 'task' &&
                    (!item.sharedTask?.assignee_user_id ||
                      !userId ||
                      item.sharedTask?.assignee_user_id === userId ||
                      isOrgAdmin);

                  if (isOwnTask) {
                    const isArchived = item.task.archived_at !== null;
                    return (
                      <SwipeableTaskCard
                        key={item.task.id}
                        task={item.task}
                        onArchive={handleSwipeArchive}
                        isArchived={isArchived}
                        disabled={item.task.is_remote}
                      >
                        <TaskCard
                          task={item.task}
                          index={index}
                          status={status}
                          onViewDetails={onViewTaskDetails}
                          isOpen={selectedTaskId === item.task.id}
                          projectId={projectId}
                          sharedTask={item.sharedTask}
                        />
                      </SwipeableTaskCard>
                    );
                  }

                  const sharedTask =
                    item.type === 'shared' ? item.task : item.sharedTask!;

                  return (
                    <SharedTaskCard
                      key={`shared-${item.task.id}`}
                      task={sharedTask}
                      index={index}
                      status={status}
                      isSelected={selectedSharedTaskId === item.task.id}
                      onViewDetails={onViewSharedTask}
                      isOrgAdmin={isOrgAdmin}
                    />
                  );
                })}
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

export default MobileKanbanBoard;
