/**
 * Message Queue API namespace - In-memory queue for follow-up messages.
 */

import type {
  QueuedMessage,
  AddQueuedMessageRequest,
  UpdateQueuedMessageRequest,
  ReorderQueuedMessagesRequest,
} from 'shared/types';
import { makeRequest, handleApiResponse } from './utils';

export const messageQueueApi = {
  /**
   * List all queued messages for a task attempt.
   *
   * @param attemptId - The task attempt ID
   */
  list: async (attemptId: string): Promise<QueuedMessage[]> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/message-queue`
    );
    return handleApiResponse<QueuedMessage[]>(response);
  },

  /**
   * Add a new message to the queue.
   *
   * @param attemptId - The task attempt ID
   * @param content - The message content
   * @param variant - Optional message variant
   */
  add: async (
    attemptId: string,
    content: string,
    variant: string | null = null
  ): Promise<QueuedMessage> => {
    const payload: AddQueuedMessageRequest = { content, variant };
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/message-queue`,
      {
        method: 'POST',
        body: JSON.stringify(payload),
      }
    );
    return handleApiResponse<QueuedMessage>(response);
  },

  /**
   * Update an existing queued message.
   *
   * @param attemptId - The task attempt ID
   * @param messageId - The message ID to update
   * @param content - New message content (or null to keep existing)
   * @param variant - New message variant (or null to keep existing)
   */
  update: async (
    attemptId: string,
    messageId: string,
    content: string | null,
    variant: string | null = null
  ): Promise<QueuedMessage> => {
    const payload: UpdateQueuedMessageRequest = { content, variant };
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/message-queue/${messageId}`,
      {
        method: 'PUT',
        body: JSON.stringify(payload),
      }
    );
    return handleApiResponse<QueuedMessage>(response);
  },

  /**
   * Remove a message from the queue.
   *
   * @param attemptId - The task attempt ID
   * @param messageId - The message ID to remove
   */
  remove: async (attemptId: string, messageId: string): Promise<void> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/message-queue/${messageId}`,
      {
        method: 'DELETE',
      }
    );
    return handleApiResponse<void>(response);
  },

  /**
   * Reorder messages in the queue.
   *
   * @param attemptId - The task attempt ID
   * @param messageIds - Array of message IDs in the desired order
   */
  reorder: async (
    attemptId: string,
    messageIds: string[]
  ): Promise<QueuedMessage[]> => {
    const payload: ReorderQueuedMessagesRequest = { message_ids: messageIds };
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/message-queue/reorder`,
      {
        method: 'POST',
        body: JSON.stringify(payload),
      }
    );
    return handleApiResponse<QueuedMessage[]>(response);
  },

  /**
   * Clear all messages from the queue.
   *
   * @param attemptId - The task attempt ID
   */
  clear: async (attemptId: string): Promise<void> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/message-queue`,
      {
        method: 'DELETE',
      }
    );
    return handleApiResponse<void>(response);
  },
};
