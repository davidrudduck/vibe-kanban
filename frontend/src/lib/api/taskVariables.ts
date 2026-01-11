/**
 * Task Variables API namespace - CRUD and operations for task variables.
 */

import type {
  TaskVariable,
  ResolvedVariable,
  CreateTaskVariable,
  UpdateTaskVariable,
  PreviewExpansionRequest,
  PreviewExpansionResponse,
} from 'shared/types';
import { makeRequest, handleApiResponse } from './utils';

// Task Variables APIs
export const taskVariablesApi = {
  /**
   * Get task's own variables (not including inherited).
   */
  list: async (taskId: string): Promise<TaskVariable[]> => {
    const response = await makeRequest(`/api/tasks/${taskId}/variables`);
    return handleApiResponse<TaskVariable[]>(response);
  },

  /**
   * Get all resolved variables (including inherited from parent tasks).
   * Child variables override parent variables with the same name.
   */
  listResolved: async (taskId: string): Promise<ResolvedVariable[]> => {
    const response = await makeRequest(
      `/api/tasks/${taskId}/variables/resolved`
    );
    return handleApiResponse<ResolvedVariable[]>(response);
  },

  /**
   * Create a new variable for a task.
   * Variable name must match [A-Z][A-Z0-9_]* pattern.
   */
  create: async (
    taskId: string,
    data: CreateTaskVariable
  ): Promise<TaskVariable> => {
    const response = await makeRequest(`/api/tasks/${taskId}/variables`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<TaskVariable>(response);
  },

  /**
   * Update an existing variable.
   */
  update: async (
    taskId: string,
    variableId: string,
    data: UpdateTaskVariable
  ): Promise<TaskVariable> => {
    const response = await makeRequest(
      `/api/tasks/${taskId}/variables/${variableId}`,
      {
        method: 'PUT',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<TaskVariable>(response);
  },

  /**
   * Delete a variable.
   */
  delete: async (taskId: string, variableId: string): Promise<void> => {
    const response = await makeRequest(
      `/api/tasks/${taskId}/variables/${variableId}`,
      {
        method: 'DELETE',
      }
    );
    return handleApiResponse<void>(response);
  },

  /**
   * Preview variable expansion in a text.
   * Returns expanded text and list of undefined variables.
   */
  preview: async (
    taskId: string,
    data: PreviewExpansionRequest
  ): Promise<PreviewExpansionResponse> => {
    const response = await makeRequest(
      `/api/tasks/${taskId}/variables/preview`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<PreviewExpansionResponse>(response);
  },
};
