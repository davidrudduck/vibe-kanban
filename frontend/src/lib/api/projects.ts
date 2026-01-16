/**
 * Projects API namespace - CRUD and related operations for projects.
 */

import type {
  Project,
  CreateProject,
  UpdateProject,
  GitBranch,
  SearchResult,
  ScanConfigRequest,
  ScanConfigResponse,
  MergedProjectsResponse,
  LinkToLocalFolderRequest,
  RemoteProjectMembersResponse,
  OpenEditorRequest,
  OpenEditorResponse,
} from 'shared/types';
import { makeRequest, handleApiResponse } from './utils';

// Project Management APIs
export const projectsApi = {
  getAll: async (): Promise<Project[]> => {
    const response = await makeRequest('/api/projects');
    return handleApiResponse<Project[]>(response);
  },

  getById: async (id: string): Promise<Project> => {
    const response = await makeRequest(`/api/projects/${id}`);
    return handleApiResponse<Project>(response);
  },

  create: async (data: CreateProject): Promise<Project> => {
    const response = await makeRequest('/api/projects', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Project>(response);
  },

  update: async (id: string, data: UpdateProject): Promise<Project> => {
    const response = await makeRequest(`/api/projects/${id}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Project>(response);
  },

  getRemoteMembers: async (
    projectId: string
  ): Promise<RemoteProjectMembersResponse> => {
    const response = await makeRequest(
      `/api/projects/${projectId}/remote/members`
    );
    return handleApiResponse<RemoteProjectMembersResponse>(response);
  },

  delete: async (id: string): Promise<void> => {
    const response = await makeRequest(`/api/projects/${id}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },

  openEditor: async (
    id: string,
    data: OpenEditorRequest
  ): Promise<OpenEditorResponse> => {
    const response = await makeRequest(`/api/projects/${id}/open-editor`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<OpenEditorResponse>(response);
  },

  getBranches: async (id: string): Promise<GitBranch[]> => {
    const response = await makeRequest(`/api/projects/${id}/branches`);
    return handleApiResponse<GitBranch[]>(response);
  },

  searchFiles: async (
    id: string,
    query: string,
    mode?: string,
    options?: RequestInit
  ): Promise<SearchResult[]> => {
    const modeParam = mode ? `&mode=${encodeURIComponent(mode)}` : '';
    const response = await makeRequest(
      `/api/projects/${id}/search?q=${encodeURIComponent(query)}${modeParam}`,
      options
    );
    return handleApiResponse<SearchResult[]>(response);
  },

  scanConfig: async (data: ScanConfigRequest): Promise<ScanConfigResponse> => {
    const response = await makeRequest('/api/projects/scan-config', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<ScanConfigResponse>(response);
  },

  getMerged: async (): Promise<MergedProjectsResponse> => {
    const response = await makeRequest('/api/merged-projects');
    return handleApiResponse<MergedProjectsResponse>(response);
  },

  linkLocalFolder: async (data: LinkToLocalFolderRequest): Promise<Project> => {
    const response = await makeRequest('/api/projects/link-local', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Project>(response);
  },

  // GitHub Integration
  setGitHubEnabled: async (
    projectId: string,
    data: { enabled: boolean; owner?: string; repo?: string }
  ): Promise<Project> => {
    const response = await makeRequest(`/api/projects/${projectId}/github`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Project>(response);
  },

  getGitHubCounts: async (
    projectId: string
  ): Promise<{
    open_issues: number;
    open_prs: number;
    last_synced_at: Date | null;
  }> => {
    const response = await makeRequest(
      `/api/projects/${projectId}/github/counts`
    );
    return handleApiResponse<{
      open_issues: number;
      open_prs: number;
      last_synced_at: Date | null;
    }>(response);
  },

  syncGitHubCounts: async (
    projectId: string
  ): Promise<{
    open_issues: number;
    open_prs: number;
    last_synced_at: Date | null;
  }> => {
    const response = await makeRequest(
      `/api/projects/${projectId}/github/sync`,
      {
        method: 'POST',
      }
    );
    return handleApiResponse<{
      open_issues: number;
      open_prs: number;
      last_synced_at: Date | null;
    }>(response);
  },
};
