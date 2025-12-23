import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { labelsApi } from '@/lib/api';
import type { Label } from 'shared/types';

/**
 * Hook to fetch labels for a specific task.
 * @param taskId - The task ID to fetch labels for
 * @param enabled - Whether to enable the query (default: true)
 */
export function useTaskLabels(taskId: string | undefined, enabled = true) {
  return useQuery({
    queryKey: ['taskLabels', taskId],
    queryFn: () => labelsApi.getTaskLabels(taskId!),
    enabled: enabled && !!taskId,
    staleTime: 30_000, // 30 seconds
  });
}

/**
 * Hook to manage (set) labels for a task with optimistic updates.
 */
export function useSetTaskLabels(taskId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (labelIds: string[]) =>
      labelsApi.setTaskLabels(taskId, { label_ids: labelIds }),
    onMutate: async (_labelIds) => {
      // Cancel any outgoing refetches
      await queryClient.cancelQueries({ queryKey: ['taskLabels', taskId] });

      // Snapshot the previous value
      const previousLabels = queryClient.getQueryData<Label[]>([
        'taskLabels',
        taskId,
      ]);

      // Optimistically update - we don't have the full label objects,
      // but we can at least update after the mutation succeeds
      return { previousLabels };
    },
    onError: (_err, _labelIds, context) => {
      // Roll back to previous value on error
      if (context?.previousLabels) {
        queryClient.setQueryData(
          ['taskLabels', taskId],
          context.previousLabels
        );
      }
    },
    onSuccess: (newLabels) => {
      // Update the cache with the server response
      queryClient.setQueryData(['taskLabels', taskId], newLabels);
    },
  });
}
