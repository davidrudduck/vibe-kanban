import { useMutation, useQueryClient } from '@tanstack/react-query';
import { attemptsApi } from '@/lib/api';
import type { PurgeResult } from 'shared/types';

interface UseAttemptCleanupMutationsOptions {
  onCleanupSuccess?: () => void;
  onCleanupError?: (err: unknown) => void;
  onPurgeSuccess?: (result: PurgeResult) => void;
  onPurgeError?: (err: unknown) => void;
}

/**
 * Provides mutation hooks for worktree cleanup operations on task attempts.
 *
 * - cleanup: Deletes the worktree filesystem and marks as deleted in DB
 * - purge: Removes build artifacts (target/, node_modules/, etc.) without deleting worktree
 */
export function useAttemptCleanupMutations(
  options?: UseAttemptCleanupMutationsOptions
) {
  const queryClient = useQueryClient();

  const cleanupWorktree = useMutation({
    mutationKey: ['cleanupWorktree'],
    mutationFn: (attemptId: string) => attemptsApi.cleanup(attemptId),
    onSuccess: () => {
      // Invalidate task attempts to refetch after cleanup
      queryClient.invalidateQueries({ queryKey: ['taskAttempts'] });
      queryClient.invalidateQueries({ queryKey: ['diskUsage'] });
      options?.onCleanupSuccess?.();
    },
    onError: (err) => {
      console.error('Failed to cleanup worktree:', err);
      options?.onCleanupError?.(err);
    },
  });

  const purgeArtifacts = useMutation({
    mutationKey: ['purgeArtifacts'],
    mutationFn: (attemptId: string) => attemptsApi.purge(attemptId),
    onSuccess: (result: PurgeResult) => {
      // Invalidate disk usage to refetch after purge
      queryClient.invalidateQueries({ queryKey: ['diskUsage'] });
      options?.onPurgeSuccess?.(result);
    },
    onError: (err) => {
      console.error('Failed to purge artifacts:', err);
      options?.onPurgeError?.(err);
    },
  });

  return { cleanupWorktree, purgeArtifacts };
}
