import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { taskVariablesApi } from '@/lib/api';
import type {
  CreateTaskVariable,
  PreviewExpansionRequest,
  PreviewExpansionResponse,
  ResolvedVariable,
  TaskVariable,
  UpdateTaskVariable,
} from 'shared/types';

/**
 * Query keys for task variables.
 * Used for cache invalidation and query identification.
 */
export const taskVariablesKeys = {
  all: ['taskVariables'] as const,
  byTask: (taskId: string) => ['taskVariables', taskId] as const,
  resolved: (taskId: string) => ['taskVariables', taskId, 'resolved'] as const,
};

/**
 * Hook to fetch a task's own variables (not including inherited).
 * Use this when editing variables for a specific task.
 *
 * @param taskId - The task ID to fetch variables for
 */
export function useTaskVariables(taskId?: string) {
  return useQuery<TaskVariable[]>({
    queryKey: taskVariablesKeys.byTask(taskId!),
    queryFn: () => taskVariablesApi.list(taskId!),
    enabled: !!taskId,
  });
}

/**
 * Hook to fetch all resolved variables for a task (including inherited).
 * Child variables override parent variables with the same name.
 * Use this when displaying the full set of available variables.
 *
 * @param taskId - The task ID to fetch resolved variables for
 */
export function useResolvedVariables(taskId?: string) {
  return useQuery<ResolvedVariable[]>({
    queryKey: taskVariablesKeys.resolved(taskId!),
    queryFn: () => taskVariablesApi.listResolved(taskId!),
    enabled: !!taskId,
  });
}

export interface UseTaskVariableMutationsOptions {
  onCreateSuccess?: (variable: TaskVariable) => void;
  onCreateError?: (err: unknown) => void;
  onUpdateSuccess?: (variable: TaskVariable) => void;
  onUpdateError?: (err: unknown) => void;
  onDeleteSuccess?: () => void;
  onDeleteError?: (err: unknown) => void;
}

/**
 * Hook for task variable mutations (create, update, delete).
 * Automatically invalidates related queries on success.
 *
 * @param taskId - The task ID to manage variables for
 * @param options - Optional callbacks for mutation events
 */
export function useTaskVariableMutations(
  taskId: string,
  options?: UseTaskVariableMutationsOptions
) {
  const queryClient = useQueryClient();

  const invalidateQueries = () => {
    // Invalidate own variables
    queryClient.invalidateQueries({
      queryKey: taskVariablesKeys.byTask(taskId),
    });
    // Invalidate resolved variables (as inheritance may have changed)
    queryClient.invalidateQueries({
      queryKey: taskVariablesKeys.resolved(taskId),
    });
    // Invalidate all task variables (for child tasks that inherit from this one)
    queryClient.invalidateQueries({
      queryKey: taskVariablesKeys.all,
    });
  };

  const createVariable = useMutation({
    mutationFn: (data: CreateTaskVariable) =>
      taskVariablesApi.create(taskId, data),
    onSuccess: (variable: TaskVariable) => {
      invalidateQueries();
      options?.onCreateSuccess?.(variable);
    },
    onError: (err) => {
      console.error('Failed to create task variable:', err);
      options?.onCreateError?.(err);
    },
  });

  const updateVariable = useMutation({
    mutationFn: ({
      variableId,
      data,
    }: {
      variableId: string;
      data: UpdateTaskVariable;
    }) => taskVariablesApi.update(taskId, variableId, data),
    onSuccess: (variable: TaskVariable) => {
      invalidateQueries();
      options?.onUpdateSuccess?.(variable);
    },
    onError: (err) => {
      console.error('Failed to update task variable:', err);
      options?.onUpdateError?.(err);
    },
  });

  const deleteVariable = useMutation({
    mutationFn: (variableId: string) =>
      taskVariablesApi.delete(taskId, variableId),
    onSuccess: () => {
      invalidateQueries();
      options?.onDeleteSuccess?.();
    },
    onError: (err) => {
      console.error('Failed to delete task variable:', err);
      options?.onDeleteError?.(err);
    },
  });

  return {
    createVariable,
    updateVariable,
    deleteVariable,
  };
}

/**
 * Hook for previewing variable expansion in text.
 * Returns expanded text and list of undefined variables.
 *
 * @param taskId - The task ID context for variable resolution
 */
export function usePreviewExpansion(taskId: string) {
  return useMutation<
    PreviewExpansionResponse,
    unknown,
    PreviewExpansionRequest
  >({
    mutationFn: (data: PreviewExpansionRequest) =>
      taskVariablesApi.preview(taskId, data),
    onError: (err) => {
      console.error('Failed to preview variable expansion:', err);
    },
  });
}
