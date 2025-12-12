import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useNavigateWithSearch } from '@/hooks';
import { tasksApi } from '@/lib/api';
import { paths } from '@/lib/paths';
import { taskRelationshipsKeys } from '@/hooks/useTaskRelationships';
import {
  useTaskOptimistic,
  getOptimisticCallback,
} from '@/contexts/TaskOptimisticContext';
import type {
  CreateTask,
  CreateAndStartTaskRequest,
  Task,
  TaskWithAttemptStatus,
  UpdateTask,
} from 'shared/types';

export interface UseTaskMutationsOptions {
  /**
   * Callback to optimistically add a task to local state.
   * Call this with the created task for instant UI feedback while
   * waiting for WebSocket broadcast.
   * If not provided, will use TaskOptimisticContext if available.
   */
  onTaskCreatedOptimistically?: (task: TaskWithAttemptStatus) => void;
}

export function useTaskMutations(
  projectId?: string,
  options?: UseTaskMutationsOptions
) {
  const queryClient = useQueryClient();
  const navigate = useNavigateWithSearch();
  const taskOptimisticContext = useTaskOptimistic();

  // Use provided callback, fall back to context, or fall back to global registry
  // The global registry is needed for modals that render outside the context
  const addTaskOptimistically =
    options?.onTaskCreatedOptimistically ??
    taskOptimisticContext?.addTaskOptimistically ??
    (projectId ? getOptimisticCallback(projectId) : undefined);

  const invalidateQueries = (taskId?: string) => {
    queryClient.invalidateQueries({ queryKey: ['tasks', projectId] });
    if (taskId) {
      queryClient.invalidateQueries({ queryKey: ['task', taskId] });
    }
  };

  const createTask = useMutation({
    mutationFn: (data: CreateTask) => tasksApi.create(data),
    onSuccess: (createdTask: Task) => {
      invalidateQueries();
      // Optimistically add task to local state for instant UI feedback
      // The task from REST API is a Task, but we need TaskWithAttemptStatus
      // For newly created tasks, attempt-related fields are all false/empty
      if (addTaskOptimistically) {
        addTaskOptimistically({
          ...createdTask,
          has_in_progress_attempt: false,
          has_merged_attempt: false,
          last_attempt_failed: false,
          executor: '',
        });
      }
      // Invalidate parent's relationships cache if this is a subtask
      if (createdTask.parent_task_attempt) {
        queryClient.invalidateQueries({
          queryKey: taskRelationshipsKeys.byAttempt(
            createdTask.parent_task_attempt
          ),
        });
      }
      if (projectId) {
        navigate(`${paths.task(projectId, createdTask.id)}/attempts/latest`);
      }
    },
    onError: (err) => {
      console.error('Failed to create task:', err);
    },
  });

  const createAndStart = useMutation({
    mutationFn: (data: CreateAndStartTaskRequest) =>
      tasksApi.createAndStart(data),
    onSuccess: (createdTask: TaskWithAttemptStatus) => {
      invalidateQueries();
      // Optimistically add task to local state for instant UI feedback
      if (addTaskOptimistically) {
        addTaskOptimistically(createdTask);
      }
      // Invalidate parent's relationships cache if this is a subtask
      if (createdTask.parent_task_attempt) {
        queryClient.invalidateQueries({
          queryKey: taskRelationshipsKeys.byAttempt(
            createdTask.parent_task_attempt
          ),
        });
      }
      if (projectId) {
        navigate(`${paths.task(projectId, createdTask.id)}/attempts/latest`);
      }
    },
    onError: (err) => {
      console.error('Failed to create and start task:', err);
    },
  });

  const updateTask = useMutation({
    mutationFn: ({ taskId, data }: { taskId: string; data: UpdateTask }) =>
      tasksApi.update(taskId, data),
    onSuccess: (updatedTask: Task) => {
      invalidateQueries(updatedTask.id);
    },
    onError: (err) => {
      console.error('Failed to update task:', err);
    },
  });

  const deleteTask = useMutation({
    mutationFn: (taskId: string) => tasksApi.delete(taskId),
    onSuccess: (_: unknown, taskId: string) => {
      invalidateQueries(taskId);
      // Remove single-task cache entry to avoid stale data flashes
      queryClient.removeQueries({ queryKey: ['task', taskId], exact: true });
      // Invalidate all task relationships caches (safe approach since we don't know parent)
      queryClient.invalidateQueries({ queryKey: taskRelationshipsKeys.all });
    },
    onError: (err) => {
      console.error('Failed to delete task:', err);
    },
  });

  const shareTask = useMutation({
    mutationFn: (taskId: string) => tasksApi.share(taskId),
    onError: (err) => {
      console.error('Failed to share task:', err);
    },
  });

  const unshareSharedTask = useMutation({
    mutationFn: (sharedTaskId: string) => tasksApi.unshare(sharedTaskId),
    onSuccess: () => {
      invalidateQueries();
    },
    onError: (err) => {
      console.error('Failed to unshare task:', err);
    },
  });

  return {
    createTask,
    createAndStart,
    updateTask,
    deleteTask,
    shareTask,
    stopShareTask: unshareSharedTask,
  };
}
