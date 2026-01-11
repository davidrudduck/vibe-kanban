/**
 * Swarm Projects API namespace.
 */

import type {
  CreateSwarmProjectRequest,
  LinkSwarmProjectNodeRequest,
  ListSwarmProjectNodesResponse,
  ListSwarmProjectsResponse,
  MergeSwarmProjectsRequest,
  SwarmProject,
  SwarmProjectNode,
  SwarmProjectNodeResponse,
  SwarmProjectResponse,
  SwarmProjectWithNodes,
  UpdateSwarmProjectRequest,
} from '@/types/swarm';

import { handleApiResponse, makeRequest } from './utils';

export const swarmProjectsApi = {
  /**
   * List all swarm projects for an organization.
   */
  list: async (organizationId: string): Promise<SwarmProjectWithNodes[]> => {
    const response = await makeRequest(
      `/api/swarm/projects?organization_id=${encodeURIComponent(organizationId)}`
    );
    const result = await handleApiResponse<ListSwarmProjectsResponse>(response);
    return result.projects;
  },

  /**
   * Get a specific swarm project by ID.
   */
  getById: async (projectId: string): Promise<SwarmProject> => {
    const response = await makeRequest(`/api/swarm/projects/${projectId}`);
    const result = await handleApiResponse<SwarmProjectResponse>(response);
    return result.project;
  },

  /**
   * Create a new swarm project.
   */
  create: async (data: CreateSwarmProjectRequest): Promise<SwarmProject> => {
    const response = await makeRequest('/api/swarm/projects', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    const result = await handleApiResponse<SwarmProjectResponse>(response);
    return result.project;
  },

  /**
   * Update an existing swarm project.
   */
  update: async (
    projectId: string,
    data: UpdateSwarmProjectRequest
  ): Promise<SwarmProject> => {
    const response = await makeRequest(`/api/swarm/projects/${projectId}`, {
      method: 'PATCH',
      body: JSON.stringify(data),
    });
    const result = await handleApiResponse<SwarmProjectResponse>(response);
    return result.project;
  },

  /**
   * Delete a swarm project.
   */
  delete: async (projectId: string): Promise<void> => {
    const response = await makeRequest(`/api/swarm/projects/${projectId}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },

  /**
   * Merge a source swarm project into a target project.
   * The source project is deleted and all node links are transferred to the target.
   */
  merge: async (
    targetProjectId: string,
    data: MergeSwarmProjectsRequest
  ): Promise<SwarmProject> => {
    const response = await makeRequest(
      `/api/swarm/projects/${targetProjectId}/merge`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    const result = await handleApiResponse<SwarmProjectResponse>(response);
    return result.project;
  },

  /**
   * List all nodes linked to a swarm project.
   */
  listNodes: async (projectId: string): Promise<SwarmProjectNode[]> => {
    const response = await makeRequest(
      `/api/swarm/projects/${projectId}/nodes`
    );
    const result =
      await handleApiResponse<ListSwarmProjectNodesResponse>(response);
    return result.nodes;
  },

  /**
   * Link a node project to a swarm project.
   */
  linkNode: async (
    projectId: string,
    data: LinkSwarmProjectNodeRequest
  ): Promise<SwarmProjectNode> => {
    const response = await makeRequest(
      `/api/swarm/projects/${projectId}/nodes`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    const result = await handleApiResponse<SwarmProjectNodeResponse>(response);
    return result.link;
  },

  /**
   * Unlink a node from a swarm project.
   */
  unlinkNode: async (projectId: string, nodeId: string): Promise<void> => {
    const response = await makeRequest(
      `/api/swarm/projects/${projectId}/nodes/${nodeId}`,
      {
        method: 'DELETE',
      }
    );
    return handleApiResponse<void>(response);
  },
};
