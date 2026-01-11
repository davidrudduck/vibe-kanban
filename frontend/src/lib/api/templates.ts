/**
 * Templates API - Template management for @mentions in descriptions.
 */

import {
  Template,
  TemplateSearchParams,
  CreateTemplate,
  UpdateTemplate,
} from 'shared/types';

import { makeRequest, handleApiResponse } from './utils';

/**
 * Templates API namespace - Used for @mentions in descriptions.
 */
export const templatesApi = {
  list: async (params?: TemplateSearchParams): Promise<Template[]> => {
    const queryParam = params?.search
      ? `?search=${encodeURIComponent(params.search)}`
      : '';
    const response = await makeRequest(`/api/templates${queryParam}`);
    return handleApiResponse<Template[]>(response);
  },

  create: async (data: CreateTemplate): Promise<Template> => {
    const response = await makeRequest('/api/templates', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Template>(response);
  },

  update: async (
    templateId: string,
    data: UpdateTemplate
  ): Promise<Template> => {
    const response = await makeRequest(`/api/templates/${templateId}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Template>(response);
  },

  delete: async (templateId: string): Promise<void> => {
    const response = await makeRequest(`/api/templates/${templateId}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },
};
