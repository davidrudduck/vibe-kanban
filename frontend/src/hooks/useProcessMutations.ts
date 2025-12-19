import { useMutation, useQueryClient } from '@tanstack/react-query';
import { processesApi } from '@/lib/api';
import type { KillScope, KillResult } from 'shared/types';

interface UseProcessMutationsOptions {
  onKillSuccess?: (result: KillResult) => void;
  onKillError?: (err: unknown) => void;
}

export function useProcessMutations(options?: UseProcessMutationsOptions) {
  const queryClient = useQueryClient();

  const killProcesses = useMutation({
    mutationKey: ['killProcesses'],
    mutationFn: ({ scope, force }: { scope: KillScope; force?: boolean }) =>
      processesApi.kill(scope, force),
    onSuccess: (result: KillResult) => {
      // Invalidate processes list to refetch after kill
      queryClient.invalidateQueries({ queryKey: ['processes'] });
      options?.onKillSuccess?.(result);
    },
    onError: (err) => {
      console.error('Failed to kill processes:', err);
      options?.onKillError?.(err);
    },
  });

  return { killProcesses };
}
