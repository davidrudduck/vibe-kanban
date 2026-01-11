/**
 * Processes API namespace - Process management endpoints.
 */

import type {
  ProcessInfo,
  ProcessFilter,
  KillScope,
  KillResult,
} from 'shared/types';
import { makeRequest, handleApiResponse } from './utils';

export const processesApi = {
  /**
   * List all vibe-kanban related processes with optional filtering.
   */
  list: async (filter?: ProcessFilter): Promise<ProcessInfo[]> => {
    const params = new URLSearchParams();
    if (filter?.project_id) {
      params.set('project_id', filter.project_id);
    }
    if (filter?.task_id) {
      params.set('task_id', filter.task_id);
    }
    if (filter?.task_attempt_id) {
      params.set('task_attempt_id', filter.task_attempt_id);
    }
    if (filter?.executors_only) {
      params.set('executors_only', 'true');
    }
    const queryString = params.toString();
    const url = queryString
      ? `/api/processes?${queryString}`
      : '/api/processes';
    const response = await makeRequest(url);
    return handleApiResponse<ProcessInfo[]>(response);
  },

  /**
   * Kill processes by scope.
   */
  kill: async (
    scope: KillScope,
    force: boolean = false
  ): Promise<KillResult> => {
    const response = await makeRequest('/api/processes/kill', {
      method: 'POST',
      body: JSON.stringify({ scope, force }),
    });
    return handleApiResponse<KillResult>(response);
  },
};
