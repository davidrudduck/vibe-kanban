import { useCallback, useMemo, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { executionProcessesApi } from '@/lib/api';
import { useMessageQueue } from './useMessageQueue';
import type { QueuedMessage } from 'shared/types';

/**
 * Extended message queue hook that supports live injection into running processes.
 *
 * When a process is running and `addAndInject` is called:
 * 1. The message is added to the backend queue (for persistence/retry)
 * 2. The message is immediately injected into the running process via stdin
 *
 * If injection fails, the message remains in the queue and will be consumed
 * when the current execution completes.
 *
 * @param attemptId - The task attempt ID for the message queue
 * @param runningProcessId - The ID of the currently running execution process (if any)
 */
export function useMessageQueueInjection(
  attemptId: string | undefined,
  runningProcessId: string | undefined
) {
  const queryClient = useQueryClient();
  const queryKey = useMemo(() => ['messageQueue', attemptId], [attemptId]);
  const messageQueue = useMessageQueue(attemptId);
  const { addMessage, removeMessage } = messageQueue;
  const [isInjecting, setIsInjecting] = useState(false);
  const [lastInjectionError, setLastInjectionError] = useState<Error | null>(
    null
  );

  /**
   * Add a message to the queue and optionally inject it into a running process.
   *
   * When injection succeeds, the message is automatically removed from the queue
   * since it has been delivered directly to the running process.
   *
   * @param content - The message content
   * @param variant - Optional message variant (e.g., 'plan')
   * @returns Object with `queued` (true if message remains in queue) and `injected` (true if live injection worked)
   */
  const addAndInject = useCallback(
    async (
      content: string,
      variant: string | null = null
    ): Promise<{ queued: boolean; injected: boolean }> => {
      setLastInjectionError(null);

      // Always add to queue first (for persistence and retry)
      const message = await addMessage(content, variant);

      // If process is running, try to inject immediately
      if (runningProcessId) {
        setIsInjecting(true);
        try {
          const result = await executionProcessesApi.injectMessage(
            runningProcessId,
            content
          );

          if (result.injected) {
            // Successfully injected - remove from queue since message was delivered
            await removeMessage(message.id);
            // Immediately update cache to prevent race condition with invalidateQueries
            // The removeMessage mutation's onSettled may cause a brief flicker otherwise
            queryClient.setQueryData<QueuedMessage[]>(queryKey, (old = []) =>
              old.filter((m) => m.id !== message.id)
            );
            return { queued: false, injected: true };
          }

          // Injection returned false (process may not accept input right now)
          return { queued: true, injected: false };
        } catch (error) {
          // Log but don't throw - the message is still in queue
          console.error(
            'Failed to inject message into running process:',
            error
          );
          setLastInjectionError(
            error instanceof Error ? error : new Error(String(error))
          );
          // Message is still in queue, will be consumed on next completion
          return { queued: true, injected: false };
        } finally {
          setIsInjecting(false);
        }
      }

      // No running process, just queued
      return { queued: true, injected: false };
    },
    [addMessage, removeMessage, runningProcessId, queryClient, queryKey]
  );

  /**
   * Inject a message into the running process without adding to queue.
   * Use this when you want to send a one-off message that shouldn't persist.
   *
   * @param content - The message content
   * @returns Object with `injected: boolean`
   */
  const injectOnly = useCallback(
    async (content: string): Promise<{ injected: boolean }> => {
      if (!runningProcessId) {
        return { injected: false };
      }

      setIsInjecting(true);
      setLastInjectionError(null);

      try {
        const result = await executionProcessesApi.injectMessage(
          runningProcessId,
          content
        );
        return result;
      } catch (error) {
        console.error('Failed to inject message:', error);
        setLastInjectionError(
          error instanceof Error ? error : new Error(String(error))
        );
        return { injected: false };
      } finally {
        setIsInjecting(false);
      }
    },
    [runningProcessId]
  );

  return {
    // All properties from useMessageQueue
    ...messageQueue,

    // Injection-specific functionality
    addAndInject,
    injectOnly,
    isInjecting,
    lastInjectionError,
    canInject: !!runningProcessId,
  };
}
