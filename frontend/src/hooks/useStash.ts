import { useMutation, useQueryClient } from '@tanstack/react-query';
import { attemptsApi } from '@/lib/api';

export function useStash(
  attemptId?: string,
  onSuccess?: () => void,
  onError?: (err: unknown) => void
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (message?: string) => {
      if (!attemptId) return;
      return attemptsApi.stashChanges(attemptId, message);
    },
    onSuccess: () => {
      // Invalidate branch status after stashing
      queryClient.invalidateQueries({ queryKey: ['branchStatus', attemptId] });
      onSuccess?.();
    },
    onError: (err) => {
      console.error('Failed to stash changes:', err);
      onError?.(err);
    },
  });
}
