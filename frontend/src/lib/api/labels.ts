/**
 * Labels API - Visual task categorization.
 */

import {
  Label,
  LabelQueryParams,
  CreateLabel,
  UpdateLabel,
  SetTaskLabels,
} from 'shared/types';

import { makeRequest, handleApiResponse } from './utils';

/**
 * Labels API namespace - Visual task categorization.
 */
export const labelsApi = {
  /** List labels. If projectId provided, returns global + project-specific labels */
  list: async (params?: LabelQueryParams): Promise<Label[]> => {
    const queryParam = params?.project_id
      ? `?project_id=${encodeURIComponent(params.project_id)}`
      : '';
    const response = await makeRequest(`/api/labels${queryParam}`);
    return handleApiResponse<Label[]>(response);
  },

  get: async (labelId: string): Promise<Label> => {
    const response = await makeRequest(`/api/labels/${labelId}`);
    return handleApiResponse<Label>(response);
  },

  create: async (data: CreateLabel): Promise<Label> => {
    const response = await makeRequest('/api/labels', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Label>(response);
  },

  update: async (labelId: string, data: UpdateLabel): Promise<Label> => {
    const response = await makeRequest(`/api/labels/${labelId}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Label>(response);
  },

  delete: async (labelId: string): Promise<void> => {
    const response = await makeRequest(`/api/labels/${labelId}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },

  /** Get labels for a specific task */
  getTaskLabels: async (taskId: string): Promise<Label[]> => {
    const response = await makeRequest(`/api/tasks/${taskId}/labels`);
    return handleApiResponse<Label[]>(response);
  },

  /** Set labels for a task (replaces existing) */
  setTaskLabels: async (
    taskId: string,
    data: SetTaskLabels
  ): Promise<Label[]> => {
    const response = await makeRequest(`/api/tasks/${taskId}/labels`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Label[]>(response);
  },
};
