/**
 * Execution processes API namespace.
 */

import type { ExecutionProcess } from 'shared/types';
import { makeRequest, handleApiResponse } from './utils';

/**
 * Execution Processes API namespace for managing running processes.
 */
export const executionProcessesApi = {
  /**
   * Get details of an execution process.
   */
  getDetails: async (processId: string): Promise<ExecutionProcess> => {
    const response = await makeRequest(`/api/execution-processes/${processId}`);
    return handleApiResponse<ExecutionProcess>(response);
  },

  /**
   * Stop an execution process.
   */
  stopExecutionProcess: async (processId: string): Promise<void> => {
    const response = await makeRequest(
      `/api/execution-processes/${processId}/stop`,
      {
        method: 'POST',
      }
    );
    return handleApiResponse<void>(response);
  },

  /**
   * Inject a message into a running execution process.
   * This allows sending user messages to Claude Code agents mid-execution.
   *
   * @param processId - The execution process ID
   * @param content - The message content to inject
   * @returns Object with `injected: boolean` indicating success
   */
  injectMessage: async (
    processId: string,
    content: string
  ): Promise<{ injected: boolean }> => {
    const response = await makeRequest(
      `/api/execution-processes/${processId}/inject-message`,
      {
        method: 'POST',
        body: JSON.stringify({ content }),
      }
    );
    return handleApiResponse<{ injected: boolean }>(response);
  },
};
