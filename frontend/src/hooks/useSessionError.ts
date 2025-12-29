import { useQuery, useQueryClient } from '@tanstack/react-query';
import { attemptsApi } from '@/lib/api';

export function useSessionError(attemptId?: string) {
  const queryClient = useQueryClient();

  const query = useQuery({
    queryKey: ['sessionError', attemptId],
    queryFn: () => attemptsApi.hasSessionError(attemptId!),
    enabled: !!attemptId,
    // Don't poll - only check on mount and manual invalidation
    staleTime: Infinity,
  });

  const invalidate = () => {
    queryClient.invalidateQueries({ queryKey: ['sessionError', attemptId] });
  };

  return {
    hasSessionError: query.data ?? false,
    isLoading: query.isLoading,
    invalidate,
  };
}
