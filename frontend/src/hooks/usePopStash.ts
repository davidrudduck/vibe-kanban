import { useMutation, useQueryClient } from '@tanstack/react-query';
import { attemptsApi } from '@/lib/api';

export function usePopStash(
  attemptId?: string,
  onSuccess?: () => void,
  onError?: (err: unknown) => void
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async () => {
      if (!attemptId) return;
      return attemptsApi.popStash(attemptId);
    },
    onSuccess: () => {
      // Invalidate branch status after popping stash
      queryClient.invalidateQueries({ queryKey: ['branchStatus', attemptId] });
      onSuccess?.();
    },
    onError: (err) => {
      console.error('Failed to pop stash:', err);
      onError?.(err);
    },
  });
}
