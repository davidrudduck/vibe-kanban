import { useQuery } from '@tanstack/react-query';
import { diagnosticsApi } from '@/lib/api';

/**
 * Hook to fetch worktree disk usage statistics.
 *
 * Returns information about total space used by worktrees,
 * the count of worktrees, and the largest worktrees.
 */
export function useDiskUsage() {
  return useQuery({
    queryKey: ['diskUsage'],
    queryFn: () => diagnosticsApi.getDiskUsage(),
    // Refetch every 60 seconds
    refetchInterval: 60000,
    // Stale after 30 seconds
    staleTime: 30000,
  });
}
