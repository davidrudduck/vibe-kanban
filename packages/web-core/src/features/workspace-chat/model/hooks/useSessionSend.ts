import { useCallback, useState } from 'react';
import type { ExecutorConfig } from 'shared/types';
import { executionProcessesApi, queueApi, sessionsApi } from '@/shared/lib/api';
import { useCreateSession } from './useCreateSession';

interface UseSessionSendOptions {
  /** Session ID for existing sessions */
  sessionId: string | undefined;
  /** Workspace ID for creating new sessions */
  workspaceId: string | undefined;
  /** Whether in new session mode */
  isNewSessionMode: boolean;
  /** Callback when session is selected (to exit new session mode) */
  onSelectSession?: (sessionId: string) => void;
  /** Unified executor config (executor + variant + overrides) */
  executorConfig?: ExecutorConfig | null;
  /**
   * ID of the currently-running execution process for this session.
   * When set, the hook will attempt live injection before falling back
   * to a queued follow-up.
   */
  runningExecutionProcessId?: string | null;
  /**
   * Whether ANY execution process is running (including non-codingagent
   * phases such as setupscript or cleanupscript).
   *
   * When true but `runningExecutionProcessId` is null, we queue the
   * message instead of spawning a new execution via followUp, so we
   * don't launch a second concurrent execution while setup/cleanup runs.
   */
  hasAnyRunningProcess?: boolean;
}

interface UseSessionSendResult {
  /** Send a message. Returns true on success, false on failure. */
  send: (message: string) => Promise<boolean>;
  /** Whether a send operation is in progress */
  isSending: boolean;
  /** Error message if send failed */
  error: string | null;
  /** Clear the error */
  clearError: () => void;
}

/**
 * Hook for sending messages in SessionChatBoxContainer.
 * Handles both new session creation and existing session follow-up.
 *
 * Unlike useFollowUpSend, this hook:
 * - Takes message/variant as parameters to send() (not captured in closure)
 * - Returns boolean for success/failure (caller handles cleanup)
 * - Has no prompt composition (no conflict/review/clicked markdown)
 */
export function useSessionSend({
  sessionId,
  workspaceId,
  isNewSessionMode,
  onSelectSession,
  executorConfig,
  runningExecutionProcessId,
  hasAnyRunningProcess,
}: UseSessionSendOptions): UseSessionSendResult {
  const { mutateAsync: createSession, isPending: isCreatingSession } =
    useCreateSession();
  const [isSendingFollowUp, setIsSendingFollowUp] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const send = useCallback(
    async (message: string): Promise<boolean> => {
      const trimmed = message.trim();
      if (!trimmed) return false;
      if (!executorConfig) {
        setError('No executor selected');
        return false;
      }

      setError(null);

      if (isNewSessionMode) {
        // New session flow
        if (!workspaceId) {
          setError('No workspace selected');
          return false;
        }
        try {
          const session = await createSession({
            workspaceId,
            prompt: trimmed,
            executorConfig,
          });
          onSelectSession?.(session.id);
          return true;
        } catch (e: unknown) {
          const err = e as { message?: string };
          setError(
            `Failed to create session: ${err.message ?? 'Unknown error'}`
          );
          return false;
        }
      } else {
        // Existing session flow
        if (!sessionId) return false;
        setIsSendingFollowUp(true);
        try {
          // If there is a running codingagent process, attempt live injection first.
          if (runningExecutionProcessId) {
            let processExited = false;
            try {
              const { injected } = await executionProcessesApi.injectMessage(
                runningExecutionProcessId,
                trimmed
              );
              if (injected) return true;
              // injected: false — process is alive but the executor doesn't
              // support live injection (e.g. unsupported executor type).
              // Queue for after the current turn finishes.
            } catch (e) {
              // The HTTP call failed — most likely because the process exited
              // between our status check and this call. Route to followUp so
              // we start a fresh execution rather than queuing for an event
              // that may never fire.
              console.warn(
                '[useSessionSend] inject-message failed, process may have exited — falling back to followUp:',
                e
              );
              processExited = true;
            }

            if (processExited) {
              await sessionsApi.followUp(sessionId, {
                prompt: trimmed,
                executor_config: executorConfig,
                retry_process_id: null,
                force_when_dirty: null,
                perform_git_reset: null,
                override_session_id: null,
              });
              return true;
            }

            // injected: false — queue for when the current execution finishes.
            await queueApi.queue(sessionId, {
              message: trimmed,
              executor_config: executorConfig,
            });
            return true;
          }

          // No codingagent process is running. If a non-codingagent process
          // (setupscript, cleanupscript) is active, queue rather than
          // spawning a second concurrent execution via followUp.
          if (hasAnyRunningProcess) {
            await queueApi.queue(sessionId, {
              message: trimmed,
              executor_config: executorConfig,
            });
            return true;
          }

          await sessionsApi.followUp(sessionId, {
            prompt: trimmed,
            executor_config: executorConfig,
            retry_process_id: null,
            force_when_dirty: null,
            perform_git_reset: null,
            override_session_id: null,
          });
          return true;
        } catch (e: unknown) {
          const err = e as { message?: string };
          setError(`Failed to send: ${err.message ?? 'Unknown error'}`);
          return false;
        } finally {
          setIsSendingFollowUp(false);
        }
      }
    },
    [
      sessionId,
      workspaceId,
      isNewSessionMode,
      createSession,
      onSelectSession,
      executorConfig,
      runningExecutionProcessId,
      hasAnyRunningProcess,
    ]
  );

  const clearError = useCallback(() => setError(null), []);

  return {
    send,
    isSending: isSendingFollowUp || isCreatingSession,
    error,
    clearError,
  };
}
