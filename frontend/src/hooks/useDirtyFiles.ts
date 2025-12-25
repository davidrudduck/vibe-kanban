import { useQuery } from '@tanstack/react-query';
import { attemptsApi } from '@/lib/api';

export function useDirtyFiles(attemptId?: string, enabled = true) {
  return useQuery({
    queryKey: ['dirtyFiles', attemptId],
    queryFn: async () => {
      if (!attemptId) {
        return { files: [] };
      }
      return attemptsApi.getDirtyFiles(attemptId);
    },
    enabled: !!attemptId && enabled,
    staleTime: 5000, // 5 seconds - dirty files can change frequently
  });
}
