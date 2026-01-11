/**
 * Tasks API namespace - CRUD and related operations for tasks.
 */

import type {
  AllTasksResponse,
  CreateTask,
  CreateAndStartTaskRequest,
  Task,
  UpdateTask,
  ArchiveTaskRequest,
  ArchiveTaskResponse,
  TaskWithAttemptStatus,
  CachedNodeStatus,
} from 'shared/types';
import { makeRequest, handleApiResponse } from './utils';

// Types for available nodes (for remote task attempt start)
export interface ProjectNodeInfo {
  node_id: string;
  node_name: string;
  node_status: CachedNodeStatus;
  node_public_url: string | null;
  node_project_id: string;
  local_project_id: string;
}

export interface ListProjectNodesResponse {
  nodes: ProjectNodeInfo[];
}

// Types for remote task stream connection info
export interface TaskStreamConnectionInfoResponse {
  task_id: string;
  node_id: string;
  /** The task attempt ID on the remote node (needed for streaming endpoint) */
  attempt_id: string | null;
  direct_url: string | null;
  relay_url: string;
  connection_token: string;
  expires_at: string;
}

// Task Management APIs
export const tasksApi = {
  getAll: async (): Promise<AllTasksResponse> => {
    const response = await makeRequest('/api/tasks/all');
    return handleApiResponse<AllTasksResponse>(response);
  },

  getById: async (taskId: string): Promise<Task> => {
    const response = await makeRequest(`/api/tasks/${taskId}`);
    return handleApiResponse<Task>(response);
  },

  create: async (data: CreateTask): Promise<Task> => {
    const response = await makeRequest(`/api/tasks`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Task>(response);
  },

  createAndStart: async (
    data: CreateAndStartTaskRequest
  ): Promise<TaskWithAttemptStatus> => {
    const response = await makeRequest(`/api/tasks/create-and-start`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<TaskWithAttemptStatus>(response);
  },

  update: async (taskId: string, data: UpdateTask): Promise<Task> => {
    const response = await makeRequest(`/api/tasks/${taskId}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Task>(response);
  },

  delete: async (taskId: string): Promise<void> => {
    const response = await makeRequest(`/api/tasks/${taskId}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },

  /** Get list of nodes where this task's project exists (for remote attempt start). */
  availableNodes: async (taskId: string): Promise<ListProjectNodesResponse> => {
    const response = await makeRequest(`/api/tasks/${taskId}/available-nodes`);
    return handleApiResponse<ListProjectNodesResponse>(response);
  },

  /** Get stream connection info for a remote task (to connect directly to the node). */
  streamConnectionInfo: async (
    taskId: string
  ): Promise<TaskStreamConnectionInfoResponse> => {
    const response = await makeRequest(
      `/api/tasks/${taskId}/stream-connection-info`
    );
    return handleApiResponse<TaskStreamConnectionInfoResponse>(response);
  },

  /** Archive a task (and optionally its subtasks). Cleans up worktrees. */
  archive: async (
    taskId: string,
    data: ArchiveTaskRequest
  ): Promise<ArchiveTaskResponse> => {
    const response = await makeRequest(`/api/tasks/${taskId}/archive`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<ArchiveTaskResponse>(response);
  },

  /** Unarchive a previously archived task. */
  unarchive: async (taskId: string): Promise<Task> => {
    const response = await makeRequest(`/api/tasks/${taskId}/unarchive`, {
      method: 'POST',
    });
    return handleApiResponse<Task>(response);
  },

  /** Assign or claim a Hive-synced task. */
  assign: async (taskId: string, newAssigneeUserId?: string): Promise<Task> => {
    const response = await makeRequest(`/api/tasks/${taskId}/assign`, {
      method: 'POST',
      body: JSON.stringify({ new_assignee_user_id: newAssigneeUserId }),
    });
    return handleApiResponse<Task>(response);
  },

  /** Get child tasks (subtasks) of a task. Used for archive confirmation dialog. */
  getChildren: async (taskId: string): Promise<Task[]> => {
    const response = await makeRequest(`/api/tasks/${taskId}/children`);
    return handleApiResponse<Task[]>(response);
  },
};
