import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { terminalApi, attemptsApi } from '@/lib/api';
import type { SessionInfo, CreateSessionResponse } from 'shared/types';

export interface UseTerminalSessionOptions {
  onCreateSuccess?: (response: CreateSessionResponse) => void;
  onCreateError?: (err: unknown) => void;
  onDeleteSuccess?: () => void;
  onDeleteError?: (err: unknown) => void;
}

export function useTerminalSessions() {
  return useQuery({
    queryKey: ['terminal', 'sessions'],
    queryFn: () => terminalApi.listSessions(),
  });
}

export function useTerminalSession(sessionId: string | null) {
  return useQuery({
    queryKey: ['terminal', 'session', sessionId],
    queryFn: () => terminalApi.getSession(sessionId!),
    enabled: !!sessionId,
  });
}

export function useTerminalSessionMutations(
  options?: UseTerminalSessionOptions
) {
  const queryClient = useQueryClient();

  const createSession = useMutation({
    mutationKey: ['createTerminalSession'],
    mutationFn: (workingDir: string) => terminalApi.createSession(workingDir),
    onSuccess: (response: CreateSessionResponse) => {
      queryClient.invalidateQueries({ queryKey: ['terminal', 'sessions'] });
      options?.onCreateSuccess?.(response);
    },
    onError: (err: unknown) => {
      console.error('Failed to create terminal session:', err);
      options?.onCreateError?.(err);
    },
  });

  const deleteSession = useMutation({
    mutationKey: ['deleteTerminalSession'],
    mutationFn: (sessionId: string) => terminalApi.deleteSession(sessionId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['terminal', 'sessions'] });
      options?.onDeleteSuccess?.();
    },
    onError: (err: unknown) => {
      console.error('Failed to delete terminal session:', err);
      options?.onDeleteError?.(err);
    },
  });

  return {
    createSession,
    deleteSession,
  };
}

export function useTerminalSessionForPath(
  workingDir: string | null,
  options?: { autoCreate?: boolean }
) {
  const { data: sessions } = useTerminalSessions();
  const { createSession } = useTerminalSessionMutations();

  // Find existing session for this path
  const existingSession = sessions?.find(
    (s: SessionInfo) => s.working_dir === workingDir && s.active
  );

  // Auto-create session if enabled and no existing session
  const shouldCreate =
    options?.autoCreate && workingDir && !existingSession && sessions !== undefined;

  return {
    session: existingSession,
    isLoading: sessions === undefined || createSession.isPending,
    createSession: () => {
      if (workingDir && !existingSession) {
        createSession.mutate(workingDir);
      }
    },
    shouldAutoCreate: shouldCreate,
  };
}

/**
 * Hook to get the worktree path for a task attempt.
 * Used to create a terminal session in the attempt's working directory.
 */
export function useAttemptWorktreePath(attemptId: string | null | undefined) {
  return useQuery({
    queryKey: ['attempt', 'worktree-path', attemptId],
    queryFn: () => attemptsApi.getWorktreePath(attemptId!),
    enabled: !!attemptId,
    staleTime: Infinity, // Worktree path doesn't change
  });
}
