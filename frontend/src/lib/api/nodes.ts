/**
 * Nodes API namespace (swarm/hive architecture).
 */

import type {
  CreateNodeApiKeyRequest,
  CreateNodeApiKeyResponse,
  MergeNodesResponse,
  Node,
  NodeApiKey,
  NodeProject,
} from '@/types/nodes';

import { handleApiResponse, makeRequest } from './utils';

export const nodesApi = {
  list: async (organizationId: string): Promise<Node[]> => {
    const response = await makeRequest(
      `/api/nodes?organization_id=${encodeURIComponent(organizationId)}`
    );
    return handleApiResponse<Node[]>(response);
  },

  getById: async (nodeId: string): Promise<Node> => {
    const response = await makeRequest(`/api/nodes/${nodeId}`);
    return handleApiResponse<Node>(response);
  },

  delete: async (nodeId: string): Promise<void> => {
    const response = await makeRequest(`/api/nodes/${nodeId}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },

  listProjects: async (nodeId: string): Promise<NodeProject[]> => {
    const response = await makeRequest(`/api/nodes/${nodeId}/projects`);
    return handleApiResponse<NodeProject[]>(response);
  },

  listApiKeys: async (organizationId: string): Promise<NodeApiKey[]> => {
    const response = await makeRequest(
      `/api/nodes/api-keys?organization_id=${encodeURIComponent(organizationId)}`
    );
    return handleApiResponse<NodeApiKey[]>(response);
  },

  createApiKey: async (
    data: CreateNodeApiKeyRequest
  ): Promise<CreateNodeApiKeyResponse> => {
    const response = await makeRequest('/api/nodes/api-keys', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<CreateNodeApiKeyResponse>(response);
  },

  revokeApiKey: async (keyId: string): Promise<void> => {
    const response = await makeRequest(`/api/nodes/api-keys/${keyId}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },

  /**
   * Unblock a blocked API key.
   * Requires admin access to the key's organization.
   */
  unblockApiKey: async (keyId: string): Promise<NodeApiKey> => {
    const response = await makeRequest(`/api/nodes/api-keys/${keyId}/unblock`, {
      method: 'POST',
    });
    return handleApiResponse<NodeApiKey>(response);
  },

  /**
   * Merge source node into target node.
   * Moves all projects and rebinds API keys from source to target, then deletes source.
   * Requires admin access to the source node's organization.
   */
  mergeNodes: async (
    sourceNodeId: string,
    targetNodeId: string
  ): Promise<MergeNodesResponse> => {
    const response = await makeRequest(
      `/api/nodes/${sourceNodeId}/merge-to/${targetNodeId}`,
      {
        method: 'POST',
      }
    );
    return handleApiResponse<MergeNodesResponse>(response);
  },
};
