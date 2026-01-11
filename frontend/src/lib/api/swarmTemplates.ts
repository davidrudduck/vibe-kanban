/**
 * Swarm Templates API namespace.
 */

import type {
  CreateSwarmTemplateRequest,
  ListSwarmTemplatesResponse,
  MergeSwarmTemplatesRequest,
  SwarmTemplate,
  SwarmTemplateResponse,
  UpdateSwarmTemplateRequest,
} from '@/types/swarm';

import { handleApiResponse, makeRequest } from './utils';

export const swarmTemplatesApi = {
  /**
   * List all swarm templates for an organization.
   */
  list: async (organizationId: string): Promise<SwarmTemplate[]> => {
    const response = await makeRequest(
      `/api/swarm/templates?organization_id=${encodeURIComponent(organizationId)}`
    );
    const result =
      await handleApiResponse<ListSwarmTemplatesResponse>(response);
    return result.templates;
  },

  /**
   * Get a specific swarm template by ID.
   */
  getById: async (templateId: string): Promise<SwarmTemplate> => {
    const response = await makeRequest(`/api/swarm/templates/${templateId}`);
    const result = await handleApiResponse<SwarmTemplateResponse>(response);
    return result.template;
  },

  /**
   * Create a new swarm template.
   */
  create: async (data: CreateSwarmTemplateRequest): Promise<SwarmTemplate> => {
    const response = await makeRequest('/api/swarm/templates', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    const result = await handleApiResponse<SwarmTemplateResponse>(response);
    return result.template;
  },

  /**
   * Update an existing swarm template.
   */
  update: async (
    templateId: string,
    data: UpdateSwarmTemplateRequest
  ): Promise<SwarmTemplate> => {
    const response = await makeRequest(`/api/swarm/templates/${templateId}`, {
      method: 'PATCH',
      body: JSON.stringify(data),
    });
    const result = await handleApiResponse<SwarmTemplateResponse>(response);
    return result.template;
  },

  /**
   * Delete a swarm template.
   */
  delete: async (templateId: string): Promise<void> => {
    const response = await makeRequest(`/api/swarm/templates/${templateId}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },

  /**
   * Merge a source template into a target template.
   */
  merge: async (
    targetTemplateId: string,
    data: MergeSwarmTemplatesRequest
  ): Promise<SwarmTemplate> => {
    const response = await makeRequest(
      `/api/swarm/templates/${targetTemplateId}/merge`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    const result = await handleApiResponse<SwarmTemplateResponse>(response);
    return result.template;
  },
};
