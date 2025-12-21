import { useQuery } from '@tanstack/react-query';
import { attemptsApi, tasksApi } from '@/lib/api';
import type { Task, TaskRelationships } from 'shared/types';

export const taskRelationshipsKeys = {
  all: ['taskRelationships'] as const,
  byAttempt: (attemptId: string | undefined) =>
    ['taskRelationships', attemptId] as const,
  byTask: (taskId: string | undefined) =>
    ['taskRelationships', 'task', taskId] as const,
};

type Options = {
  enabled?: boolean;
  refetchInterval?: number | false;
  staleTime?: number;
  retry?: number | false;
};

export function useTaskRelationships(attemptId?: string, opts?: Options) {
  const enabled = (opts?.enabled ?? true) && !!attemptId;

  return useQuery<TaskRelationships>({
    queryKey: taskRelationshipsKeys.byAttempt(attemptId),
    queryFn: async () => {
      const data = await attemptsApi.getChildren(attemptId!);
      return data;
    },
    enabled,
    refetchInterval: opts?.refetchInterval ?? false,
    staleTime: opts?.staleTime ?? 10_000,
    retry: opts?.retry ?? 2,
  });
}

/**
 * Fetch children (subtasks) of a task directly by task ID.
 * Used when viewing a task without an attempt selected.
 */
export function useTaskChildren(taskId?: string, opts?: Options) {
  const enabled = (opts?.enabled ?? true) && !!taskId;

  return useQuery<Task[]>({
    queryKey: taskRelationshipsKeys.byTask(taskId),
    queryFn: async () => {
      const data = await tasksApi.getChildren(taskId!);
      return data;
    },
    enabled,
    refetchInterval: opts?.refetchInterval ?? false,
    staleTime: opts?.staleTime ?? 10_000,
    retry: opts?.retry ?? 2,
  });
}
