import { useQuery } from '@tanstack/react-query';
import { attemptsApi } from '@/lib/api';

export function useBranchStatus(attemptId?: string) {
  return useQuery({
    queryKey: ['branchStatus', attemptId],
    queryFn: () => attemptsApi.getBranchStatus(attemptId!),
    enabled: !!attemptId,
    // Only poll when tab is visible to reduce unnecessary network requests
    refetchInterval: () => (document.hidden ? false : 5000),
    // Limit retries to prevent infinite error loops (e.g., when worktree doesn't exist)
    retry: 1,
    retryDelay: 1000,
  });
}
