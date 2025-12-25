import { useQuery } from '@tanstack/react-query';
import { tasksApi, type ListProjectNodesResponse } from '@/lib/api';

/**
 * Hook to fetch nodes where a task's project exists.
 * Used for selecting a node when starting a remote task attempt.
 */
export function useAvailableNodes(
  taskId: string | undefined,
  options?: { enabled?: boolean }
) {
  return useQuery<ListProjectNodesResponse>({
    queryKey: ['availableNodes', taskId],
    queryFn: () => tasksApi.availableNodes(taskId!),
    enabled: options?.enabled !== false && !!taskId,
    staleTime: 30000, // 30 seconds
  });
}
