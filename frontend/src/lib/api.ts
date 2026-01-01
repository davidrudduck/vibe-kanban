// Import all necessary types from shared types

import {
  ActivityFeed,
  AllTasksResponse,
  ApprovalStatus,
  ApiResponse,
  ArchiveTaskRequest,
  ArchiveTaskResponse,
  BackupInfo,
  BranchStatus,
  Config,
  CommitInfo,
  CreateFollowUpAttempt,
  DashboardSummary,
  EditorType,
  CreateGitHubPrRequest,
  CreateTask,
  CreateAndStartTaskRequest,
  CreateTaskAttemptBody,
  CreateTemplate,
  CreateLabel,
  CreateTaskVariable,
  DirectoryListResponse,
  DirectoryEntry,
  Direction,
  FileContentResponse,
  ExecutionProcess,
  GitBranch,
  PaginatedLogs,
  PreviewExpansionRequest,
  PreviewExpansionResponse,
  Project,
  CreateProject,
  ResolvedVariable,
  SearchResult,
  Task,
  TaskAttempt,
  TaskRelationships,
  TaskVariable,
  Template,
  TemplateSearchParams,
  Label,
  LabelQueryParams,
  SetTaskLabels,
  TaskWithAttemptStatus,
  AssignSharedTaskResponse,
  UpdateProject,
  UpdateTask,
  UpdateTemplate,
  UpdateLabel,
  UpdateTaskVariable,
  UserSystemInfo,
  UpdateRetryFollowUpDraftRequest,
  McpServerQuery,
  UpdateMcpServersBody,
  GetMcpServerResponse,
  ImageResponse,
  DraftResponse,
  UpdateFollowUpDraftRequest,
  GitOperationError,
  ApprovalResponse,
  RebaseTaskAttemptRequest,
  ChangeTargetBranchRequest,
  ChangeTargetBranchResponse,
  RenameBranchRequest,
  RenameBranchResponse,
  CheckEditorAvailabilityResponse,
  AvailabilityInfo,
  BaseCodingAgent,
  RunAgentSetupRequest,
  RunAgentSetupResponse,
  GhCliSetupError,
  StatusResponse,
  ListOrganizationsResponse,
  OrganizationMemberWithProfile,
  ListMembersResponse,
  RemoteProjectMembersResponse,
  CreateOrganizationRequest,
  CreateOrganizationResponse,
  CreateInvitationRequest,
  CreateInvitationResponse,
  RevokeInvitationRequest,
  UpdateMemberRoleRequest,
  LinkToLocalFolderRequest,
  UpdateMemberRoleResponse,
  Invitation,
  RemoteProject,
  ListInvitationsResponse,
  CommitCompareResult,
  OpenEditorResponse,
  OpenEditorRequest,
  CreatePrError,
  PushError,
  ScanConfigRequest,
  ScanConfigResponse,
  UnifiedProjectsResponse,
  MergedProjectsResponse,
  CachedNodeStatus,
  ProcessInfo,
  ProcessFilter,
  KillScope,
  KillResult,
  SessionInfo,
  CreateSessionResponse,
  WorktreePathResponse,
  DirtyFilesResponse,
  StashChangesRequest,
  StashChangesResponse,
  PurgeResult,
  DiskUsageStats,
  FixSessionsResponse,
  QueuedMessage,
  AddQueuedMessageRequest,
  UpdateQueuedMessageRequest,
  ReorderQueuedMessagesRequest,
} from 'shared/types';

// Re-export types for convenience
export type {
  UpdateFollowUpDraftRequest,
  UpdateRetryFollowUpDraftRequest,
} from 'shared/types';

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

class ApiError<E = unknown> extends Error {
  public status?: number;
  public error_data?: E;

  constructor(
    message: string,
    public statusCode?: number,
    public response?: Response,
    error_data?: E
  ) {
    super(message);
    this.name = 'ApiError';
    this.status = statusCode;
    this.error_data = error_data;
  }
}

const REQUEST_TIMEOUT_MS = 30000; // 30 seconds

const makeRequest = async (url: string, options: RequestInit = {}) => {
  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);

  const headers = new Headers(options.headers ?? {});
  if (!headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }

  try {
    return await fetch(url, {
      ...options,
      headers,
      signal: options.signal
        ? // If caller provided a signal, combine with timeout
          anySignal([options.signal, controller.signal])
        : controller.signal,
    });
  } finally {
    clearTimeout(timeoutId);
  }
};

// Helper to combine multiple AbortSignals (first one to abort wins)
function anySignal(signals: AbortSignal[]): AbortSignal {
  const controller = new AbortController();
  for (const signal of signals) {
    if (signal.aborted) {
      controller.abort(signal.reason);
      break;
    }
    signal.addEventListener('abort', () => controller.abort(signal.reason), {
      once: true,
    });
  }
  return controller.signal;
}

export type Ok<T> = { success: true; data: T };
export type Err<E> = { success: false; error: E | undefined; message?: string };

// Result type for endpoints that need typed errors
export type Result<T, E> = Ok<T> | Err<E>;

// Special handler for Result-returning endpoints
const handleApiResponseAsResult = async <T, E>(
  response: Response
): Promise<Result<T, E>> => {
  if (!response.ok) {
    // HTTP error - no structured error data
    let errorMessage = `Request failed with status ${response.status}`;

    try {
      const errorData = await response.json();
      if (errorData.message) {
        errorMessage = errorData.message;
      }
    } catch {
      errorMessage = response.statusText || errorMessage;
    }

    return {
      success: false,
      error: undefined,
      message: errorMessage,
    };
  }

  const result: ApiResponse<T, E> = await response.json();

  if (!result.success) {
    return {
      success: false,
      error: result.error_data || undefined,
      message: result.message || undefined,
    };
  }

  return { success: true, data: result.data as T };
};

const handleApiResponse = async <T, E = T>(response: Response): Promise<T> => {
  if (!response.ok) {
    let errorMessage = `Request failed with status ${response.status}`;

    try {
      const errorData = await response.json();
      if (errorData.message) {
        errorMessage = errorData.message;
      }
    } catch {
      // Fallback to status text if JSON parsing fails
      errorMessage = response.statusText || errorMessage;
    }

    console.error('[API Error]', {
      message: errorMessage,
      status: response.status,
      response,
      endpoint: response.url,
      timestamp: new Date().toISOString(),
    });
    throw new ApiError<E>(errorMessage, response.status, response);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  const result: ApiResponse<T, E> = await response.json();

  if (!result.success) {
    // Check for error_data first (structured errors), then fall back to message
    if (result.error_data) {
      console.error('[API Error with data]', {
        error_data: result.error_data,
        message: result.message,
        status: response.status,
        response,
        endpoint: response.url,
        timestamp: new Date().toISOString(),
      });
      // Throw a properly typed error with the error data
      throw new ApiError<E>(
        result.message || 'API request failed',
        response.status,
        response,
        result.error_data
      );
    }

    console.error('[API Error]', {
      message: result.message || 'API request failed',
      status: response.status,
      response,
      endpoint: response.url,
      timestamp: new Date().toISOString(),
    });
    throw new ApiError<E>(
      result.message || 'API request failed',
      response.status,
      response
    );
  }

  return result.data as T;
};

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

  getUnified: async (): Promise<UnifiedProjectsResponse> => {
    const response = await makeRequest('/api/unified-projects');
    return handleApiResponse<UnifiedProjectsResponse>(response);
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

  reassign: async (
    sharedTaskId: string,
    data: { new_assignee_user_id: string | null; version?: number | null }
  ): Promise<AssignSharedTaskResponse> => {
    const payload = {
      new_assignee_user_id: data.new_assignee_user_id,
      version: data.version ?? null,
    };

    const response = await makeRequest(
      `/api/shared-tasks/${sharedTaskId}/assign`,
      {
        method: 'POST',
        body: JSON.stringify(payload),
      }
    );

    return handleApiResponse<AssignSharedTaskResponse>(response);
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

  /** Get child tasks (subtasks) of a task. Used for archive confirmation dialog. */
  getChildren: async (taskId: string): Promise<Task[]> => {
    const response = await makeRequest(`/api/tasks/${taskId}/children`);
    return handleApiResponse<Task[]>(response);
  },
};

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

// Task Attempts APIs
export const attemptsApi = {
  getChildren: async (attemptId: string): Promise<TaskRelationships> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/children`
    );
    return handleApiResponse<TaskRelationships>(response);
  },

  getAll: async (taskId: string): Promise<TaskAttempt[]> => {
    const response = await makeRequest(`/api/task-attempts?task_id=${taskId}`);
    return handleApiResponse<TaskAttempt[]>(response);
  },

  get: async (attemptId: string): Promise<TaskAttempt> => {
    const response = await makeRequest(`/api/task-attempts/${attemptId}`);
    return handleApiResponse<TaskAttempt>(response);
  },

  create: async (data: CreateTaskAttemptBody): Promise<TaskAttempt> => {
    const response = await makeRequest(`/api/task-attempts`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<TaskAttempt>(response);
  },

  stop: async (attemptId: string): Promise<void> => {
    const response = await makeRequest(`/api/task-attempts/${attemptId}/stop`, {
      method: 'POST',
    });
    return handleApiResponse<void>(response);
  },

  followUp: async (
    attemptId: string,
    data: CreateFollowUpAttempt
  ): Promise<void> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/follow-up`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<void>(response);
  },

  runAgentSetup: async (
    attemptId: string,
    data: RunAgentSetupRequest
  ): Promise<RunAgentSetupResponse> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/run-agent-setup`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<RunAgentSetupResponse>(response);
  },

  getDraft: async (
    attemptId: string,
    type: 'follow_up' | 'retry'
  ): Promise<DraftResponse> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/draft?type=${encodeURIComponent(type)}`
    );
    return handleApiResponse<DraftResponse>(response);
  },

  saveDraft: async (
    attemptId: string,
    type: 'follow_up' | 'retry',
    data: UpdateFollowUpDraftRequest | UpdateRetryFollowUpDraftRequest
  ): Promise<DraftResponse> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/draft?type=${encodeURIComponent(type)}`,
      {
        method: 'PUT',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<DraftResponse>(response);
  },

  deleteDraft: async (
    attemptId: string,
    type: 'follow_up' | 'retry'
  ): Promise<void> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/draft?type=${encodeURIComponent(type)}`,
      { method: 'DELETE' }
    );
    return handleApiResponse<void>(response);
  },

  setDraftQueue: async (
    attemptId: string,
    queued: boolean,
    expectedQueued?: boolean,
    expectedVersion?: number,
    type: 'follow_up' | 'retry' = 'follow_up'
  ): Promise<DraftResponse> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/draft/queue?type=${encodeURIComponent(type)}`,
      {
        method: 'POST',
        body: JSON.stringify({
          queued,
          expected_queued: expectedQueued,
          expected_version: expectedVersion,
        }),
      }
    );
    return handleApiResponse<DraftResponse>(response);
  },

  openEditor: async (
    attemptId: string,
    data: OpenEditorRequest
  ): Promise<OpenEditorResponse> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/open-editor`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<OpenEditorResponse>(response);
  },

  getBranchStatus: async (attemptId: string): Promise<BranchStatus> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/branch-status`
    );
    return handleApiResponse<BranchStatus>(response);
  },

  merge: async (attemptId: string): Promise<void> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/merge`,
      {
        method: 'POST',
      }
    );
    return handleApiResponse<void>(response);
  },

  push: async (attemptId: string): Promise<Result<void, PushError>> => {
    const response = await makeRequest(`/api/task-attempts/${attemptId}/push`, {
      method: 'POST',
    });
    return handleApiResponseAsResult<void, PushError>(response);
  },

  forcePush: async (attemptId: string): Promise<Result<void, PushError>> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/push/force`,
      {
        method: 'POST',
      }
    );
    return handleApiResponseAsResult<void, PushError>(response);
  },

  rebase: async (
    attemptId: string,
    data: RebaseTaskAttemptRequest
  ): Promise<Result<void, GitOperationError>> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/rebase`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponseAsResult<void, GitOperationError>(response);
  },

  change_target_branch: async (
    attemptId: string,
    data: ChangeTargetBranchRequest
  ): Promise<ChangeTargetBranchResponse> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/change-target-branch`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<ChangeTargetBranchResponse>(response);
  },

  renameBranch: async (
    attemptId: string,
    newBranchName: string
  ): Promise<RenameBranchResponse> => {
    const payload: RenameBranchRequest = {
      new_branch_name: newBranchName,
    };
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/rename-branch`,
      {
        method: 'POST',
        body: JSON.stringify(payload),
      }
    );
    return handleApiResponse<RenameBranchResponse>(response);
  },

  abortConflicts: async (attemptId: string): Promise<void> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/conflicts/abort`,
      {
        method: 'POST',
      }
    );
    return handleApiResponse<void>(response);
  },

  createPR: async (
    attemptId: string,
    data: CreateGitHubPrRequest
  ): Promise<Result<string, CreatePrError>> => {
    const response = await makeRequest(`/api/task-attempts/${attemptId}/pr`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponseAsResult<string, CreatePrError>(response);
  },

  startDevServer: async (attemptId: string): Promise<void> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/start-dev-server`,
      {
        method: 'POST',
      }
    );
    return handleApiResponse<void>(response);
  },

  setupGhCli: async (attemptId: string): Promise<ExecutionProcess> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/gh-cli-setup`,
      {
        method: 'POST',
      }
    );
    return handleApiResponse<ExecutionProcess, GhCliSetupError>(response);
  },

  getWorktreePath: async (attemptId: string): Promise<WorktreePathResponse> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/worktree-path`
    );
    return handleApiResponse<WorktreePathResponse>(response);
  },

  // Stash operations
  getDirtyFiles: async (attemptId: string): Promise<DirtyFilesResponse> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/stash/dirty-files`
    );
    return handleApiResponse<DirtyFilesResponse>(response);
  },

  stashChanges: async (
    attemptId: string,
    message?: string
  ): Promise<StashChangesResponse> => {
    const payload: StashChangesRequest = { message: message ?? null };
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/stash`,
      {
        method: 'POST',
        body: JSON.stringify(payload),
      }
    );
    return handleApiResponse<StashChangesResponse>(response);
  },

  popStash: async (attemptId: string): Promise<void> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/stash/pop`,
      {
        method: 'POST',
      }
    );
    return handleApiResponse<void>(response);
  },

  /** Clean up a task attempt's worktree (deletes filesystem and marks as deleted in DB). */
  cleanup: async (attemptId: string): Promise<void> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/cleanup`,
      {
        method: 'POST',
      }
    );
    return handleApiResponse<void>(response);
  },

  /** Purge build artifacts from worktree without deleting it. */
  purge: async (attemptId: string): Promise<PurgeResult> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/purge`,
      {
        method: 'POST',
      }
    );
    return handleApiResponse<PurgeResult>(response);
  },

  /** Check if the latest failed execution has a session invalid error. */
  hasSessionError: async (attemptId: string): Promise<boolean> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/has-session-error`
    );
    return handleApiResponse<boolean>(response);
  },

  /** Fix corrupted sessions by invalidating sessions from failed/killed execution processes. */
  fixSessions: async (attemptId: string): Promise<FixSessionsResponse> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/fix-sessions`,
      {
        method: 'POST',
      }
    );
    return handleApiResponse<FixSessionsResponse>(response);
  },
};

// Extra helpers
export const commitsApi = {
  getInfo: async (attemptId: string, sha: string): Promise<CommitInfo> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/commit-info?sha=${encodeURIComponent(
        sha
      )}`
    );
    return handleApiResponse<CommitInfo>(response);
  },
  compareToHead: async (
    attemptId: string,
    sha: string
  ): Promise<CommitCompareResult> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/commit-compare?sha=${encodeURIComponent(
        sha
      )}`
    );
    return handleApiResponse(response);
  },
};

// Execution Process APIs
export const executionProcessesApi = {
  getDetails: async (processId: string): Promise<ExecutionProcess> => {
    const response = await makeRequest(`/api/execution-processes/${processId}`);
    return handleApiResponse<ExecutionProcess>(response);
  },

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

// File System APIs
export const fileSystemApi = {
  list: async (path?: string): Promise<DirectoryListResponse> => {
    const queryParam = path ? `?path=${encodeURIComponent(path)}` : '';
    const response = await makeRequest(
      `/api/filesystem/directory${queryParam}`
    );
    return handleApiResponse<DirectoryListResponse>(response);
  },

  listGitRepos: async (path?: string): Promise<DirectoryEntry[]> => {
    const queryParam = path ? `?path=${encodeURIComponent(path)}` : '';
    const response = await makeRequest(
      `/api/filesystem/git-repos${queryParam}`
    );
    return handleApiResponse<DirectoryEntry[]>(response);
  },
};

// File Browser APIs (for worktree and project file browsing)
export const fileBrowserApi = {
  // List directory contents within a task attempt's worktree
  listWorktreeDirectory: async (
    attemptId: string,
    path?: string
  ): Promise<DirectoryListResponse> => {
    const queryParam = path ? `?path=${encodeURIComponent(path)}` : '';
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/files${queryParam}`
    );
    return handleApiResponse<DirectoryListResponse>(response);
  },

  // Read file content from a task attempt's worktree
  readWorktreeFile: async (
    attemptId: string,
    filePath: string
  ): Promise<FileContentResponse> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/files/${encodeURIComponent(filePath)}`
    );
    return handleApiResponse<FileContentResponse>(response);
  },

  // List directory contents within a project's git repo
  listProjectDirectory: async (
    projectId: string,
    path?: string
  ): Promise<DirectoryListResponse> => {
    const queryParam = path ? `?path=${encodeURIComponent(path)}` : '';
    const response = await makeRequest(
      `/api/projects/${projectId}/files${queryParam}`
    );
    return handleApiResponse<DirectoryListResponse>(response);
  },

  // Read file content from a project's git repo
  readProjectFile: async (
    projectId: string,
    filePath: string
  ): Promise<FileContentResponse> => {
    const response = await makeRequest(
      `/api/projects/${projectId}/files/${encodeURIComponent(filePath)}`
    );
    return handleApiResponse<FileContentResponse>(response);
  },

  // Read file content from ~/.claude/ directory (security-restricted)
  readClaudeFile: async (
    relativePath: string
  ): Promise<FileContentResponse> => {
    const response = await makeRequest(
      `/api/filesystem/claude-file?path=${encodeURIComponent(relativePath)}`
    );
    return handleApiResponse<FileContentResponse>(response);
  },
};

// Config APIs (backwards compatible)
export const configApi = {
  getConfig: async (): Promise<UserSystemInfo> => {
    const response = await makeRequest('/api/info');
    return handleApiResponse<UserSystemInfo>(response);
  },
  saveConfig: async (config: Config): Promise<Config> => {
    const response = await makeRequest('/api/config', {
      method: 'PUT',
      body: JSON.stringify(config),
    });
    return handleApiResponse<Config>(response);
  },
  checkEditorAvailability: async (
    editorType: EditorType
  ): Promise<CheckEditorAvailabilityResponse> => {
    const response = await makeRequest(
      `/api/editors/check-availability?editor_type=${encodeURIComponent(editorType)}`
    );
    return handleApiResponse<CheckEditorAvailabilityResponse>(response);
  },
  checkAgentAvailability: async (
    agent: BaseCodingAgent
  ): Promise<AvailabilityInfo> => {
    const response = await makeRequest(
      `/api/agents/check-availability?executor=${encodeURIComponent(agent)}`
    );
    return handleApiResponse<AvailabilityInfo>(response);
  },
};

// Templates APIs (renamed from Tags - used for @mentions in descriptions)
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

// Labels APIs (visual task categorization)
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

// MCP Servers APIs
export const mcpServersApi = {
  load: async (query: McpServerQuery): Promise<GetMcpServerResponse> => {
    const params = new URLSearchParams(query);
    const response = await makeRequest(`/api/mcp-config?${params.toString()}`);
    return handleApiResponse<GetMcpServerResponse>(response);
  },
  save: async (
    query: McpServerQuery,
    data: UpdateMcpServersBody
  ): Promise<void> => {
    const params = new URLSearchParams(query);
    // params.set('profile', profile);
    const response = await makeRequest(`/api/mcp-config?${params.toString()}`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    if (!response.ok) {
      const errorData = await response.json();
      console.error('[API Error] Failed to save MCP servers', {
        message: errorData.message,
        status: response.status,
        response,
        timestamp: new Date().toISOString(),
      });
      throw new ApiError(
        errorData.message || 'Failed to save MCP servers',
        response.status,
        response
      );
    }
  },
};

// Profiles API
export const profilesApi = {
  load: async (): Promise<{ content: string; path: string }> => {
    const response = await makeRequest('/api/profiles');
    return handleApiResponse<{ content: string; path: string }>(response);
  },
  save: async (content: string): Promise<string> => {
    const response = await makeRequest('/api/profiles', {
      method: 'PUT',
      body: content,
      headers: {
        'Content-Type': 'application/json',
      },
    });
    return handleApiResponse<string>(response);
  },
};

// Images API
export const imagesApi = {
  upload: async (file: File): Promise<ImageResponse> => {
    const formData = new FormData();
    formData.append('image', file);

    const response = await fetch('/api/images/upload', {
      method: 'POST',
      body: formData,
      credentials: 'include',
    });

    if (!response.ok) {
      const errorText = await response.text();
      throw new ApiError(
        `Failed to upload image: ${errorText}`,
        response.status,
        response
      );
    }

    return handleApiResponse<ImageResponse>(response);
  },

  uploadForTask: async (taskId: string, file: File): Promise<ImageResponse> => {
    const formData = new FormData();
    formData.append('image', file);

    const response = await fetch(`/api/images/task/${taskId}/upload`, {
      method: 'POST',
      body: formData,
      credentials: 'include',
    });

    if (!response.ok) {
      const errorText = await response.text();
      throw new ApiError(
        `Failed to upload image: ${errorText}`,
        response.status,
        response
      );
    }

    return handleApiResponse<ImageResponse>(response);
  },

  delete: async (imageId: string): Promise<void> => {
    const response = await makeRequest(`/api/images/${imageId}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },

  getTaskImages: async (taskId: string): Promise<ImageResponse[]> => {
    const response = await makeRequest(`/api/images/task/${taskId}`);
    return handleApiResponse<ImageResponse[]>(response);
  },

  getImageUrl: (imageId: string): string => {
    return `/api/images/${imageId}/file`;
  },
};

// Approval API
export const approvalsApi = {
  respond: async (
    approvalId: string,
    payload: ApprovalResponse,
    signal?: AbortSignal
  ): Promise<ApprovalStatus> => {
    const res = await makeRequest(`/api/approvals/${approvalId}/respond`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
      signal,
    });

    return handleApiResponse<ApprovalStatus>(res);
  },
};

// OAuth API
export const oauthApi = {
  handoffInit: async (
    provider: string,
    returnTo: string
  ): Promise<{ handoff_id: string; authorize_url: string }> => {
    const response = await makeRequest('/api/auth/handoff/init', {
      method: 'POST',
      body: JSON.stringify({ provider, return_to: returnTo }),
    });
    return handleApiResponse<{ handoff_id: string; authorize_url: string }>(
      response
    );
  },

  status: async (): Promise<StatusResponse> => {
    const response = await makeRequest('/api/auth/status');
    return handleApiResponse<StatusResponse>(response);
  },

  logout: async (): Promise<void> => {
    const response = await makeRequest('/api/auth/logout', {
      method: 'POST',
    });
    if (!response.ok) {
      throw new ApiError(
        `Logout failed with status ${response.status}`,
        response.status,
        response
      );
    }
  },
};

// Organizations API
export const organizationsApi = {
  getMembers: async (
    orgId: string
  ): Promise<OrganizationMemberWithProfile[]> => {
    const response = await makeRequest(`/api/organizations/${orgId}/members`);
    const result = await handleApiResponse<ListMembersResponse>(response);
    return result.members;
  },

  getUserOrganizations: async (): Promise<ListOrganizationsResponse> => {
    const response = await makeRequest('/api/organizations');
    return handleApiResponse<ListOrganizationsResponse>(response);
  },

  getProjects: async (orgId: string): Promise<RemoteProject[]> => {
    const response = await makeRequest(`/api/organizations/${orgId}/projects`);
    return handleApiResponse<RemoteProject[]>(response);
  },

  createOrganization: async (
    data: CreateOrganizationRequest
  ): Promise<CreateOrganizationResponse> => {
    const response = await makeRequest('/api/organizations', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(data),
    });
    return handleApiResponse<CreateOrganizationResponse>(response);
  },

  createInvitation: async (
    orgId: string,
    data: CreateInvitationRequest
  ): Promise<CreateInvitationResponse> => {
    const response = await makeRequest(
      `/api/organizations/${orgId}/invitations`,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<CreateInvitationResponse>(response);
  },

  removeMember: async (orgId: string, userId: string): Promise<void> => {
    const response = await makeRequest(
      `/api/organizations/${orgId}/members/${userId}`,
      {
        method: 'DELETE',
      }
    );
    return handleApiResponse<void>(response);
  },

  updateMemberRole: async (
    orgId: string,
    userId: string,
    data: UpdateMemberRoleRequest
  ): Promise<UpdateMemberRoleResponse> => {
    const response = await makeRequest(
      `/api/organizations/${orgId}/members/${userId}/role`,
      {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<UpdateMemberRoleResponse>(response);
  },

  listInvitations: async (orgId: string): Promise<Invitation[]> => {
    const response = await makeRequest(
      `/api/organizations/${orgId}/invitations`
    );
    const result = await handleApiResponse<ListInvitationsResponse>(response);
    return result.invitations;
  },

  revokeInvitation: async (
    orgId: string,
    invitationId: string
  ): Promise<void> => {
    const body: RevokeInvitationRequest = { invitation_id: invitationId };
    const response = await makeRequest(
      `/api/organizations/${orgId}/invitations/revoke`,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      }
    );
    return handleApiResponse<void>(response);
  },

  deleteOrganization: async (orgId: string): Promise<void> => {
    const response = await makeRequest(`/api/organizations/${orgId}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },
};

// Dashboard API
export const dashboardApi = {
  getSummary: async (): Promise<DashboardSummary> => {
    const response = await makeRequest('/api/dashboard/summary');
    return handleApiResponse<DashboardSummary>(response);
  },
  getActivityFeed: async (includeDismissed = false): Promise<ActivityFeed> => {
    const queryParam = includeDismissed ? '?include_dismissed=true' : '';
    const response = await makeRequest(`/api/dashboard/activity${queryParam}`);
    return handleApiResponse<ActivityFeed>(response);
  },
  dismissActivityItem: async (taskId: string): Promise<void> => {
    const response = await makeRequest('/api/dashboard/activity/dismiss', {
      method: 'POST',
      body: JSON.stringify({ task_id: taskId }),
    });
    return handleApiResponse<void>(response);
  },
  undismissActivityItem: async (taskId: string): Promise<void> => {
    const response = await makeRequest(
      `/api/dashboard/activity/dismiss/${encodeURIComponent(taskId)}`,
      { method: 'DELETE' }
    );
    return handleApiResponse<void>(response);
  },
};

// Nodes API (swarm/hive architecture)
import type {
  Node,
  NodeProject,
  NodeApiKey,
  CreateNodeApiKeyRequest,
  CreateNodeApiKeyResponse,
  MergeNodesResponse,
} from '@/types/nodes';

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

// === Backups API ===
export const backupsApi = {
  /**
   * List all available database backups, sorted newest first.
   */
  list: async (): Promise<BackupInfo[]> => {
    const response = await makeRequest('/api/backups');
    return handleApiResponse<BackupInfo[]>(response);
  },

  /**
   * Create a new database backup.
   */
  create: async (): Promise<BackupInfo> => {
    const response = await makeRequest('/api/backups', { method: 'POST' });
    return handleApiResponse<BackupInfo>(response);
  },

  /**
   * Delete a database backup by filename.
   */
  delete: async (filename: string): Promise<void> => {
    const response = await makeRequest(
      `/api/backups/${encodeURIComponent(filename)}`,
      {
        method: 'DELETE',
      }
    );
    return handleApiResponse<void>(response);
  },

  /**
   * Get the download URL for a backup file.
   * The browser will handle the download when navigated to this URL.
   */
  getDownloadUrl: (filename: string): string => {
    return `/api/backups/${encodeURIComponent(filename)}/download`;
  },

  /**
   * Restore database from an uploaded backup file.
   * Returns a message indicating the application needs to be restarted.
   */
  restore: async (file: File): Promise<string> => {
    const formData = new FormData();
    formData.append('backup', file);
    const response = await fetch('/api/backups/restore', {
      method: 'POST',
      body: formData,
    });
    const result: ApiResponse<string> = await response.json();
    if (!response.ok || !result.success) {
      throw new Error(result.message || 'Failed to restore backup');
    }
    return result.data!;
  },
};

// === Diagnostics API ===
export const diagnosticsApi = {
  /**
   * Get disk usage statistics for worktrees.
   * Returns total space used, worktree count, and largest worktrees.
   */
  getDiskUsage: async (): Promise<DiskUsageStats> => {
    const response = await makeRequest('/api/diagnostics/disk-usage');
    return handleApiResponse<DiskUsageStats>(response);
  },
};

// === Processes API ===
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

// === Logs API (Unified Log Access) ===
export interface LogsPaginationParams {
  limit?: number;
  cursor?: bigint;
  direction?: Direction;
}

export const logsApi = {
  /**
   * Get paginated logs for an execution process.
   * Uses cursor-based pagination for efficient scrolling.
   *
   * @param executionId - The execution process ID
   * @param params - Pagination parameters (limit, cursor, direction)
   */
  getPaginated: async (
    executionId: string,
    params?: LogsPaginationParams
  ): Promise<PaginatedLogs> => {
    const queryParams = new URLSearchParams();
    if (params?.limit !== undefined) {
      queryParams.set('limit', params.limit.toString());
    }
    if (params?.cursor !== undefined) {
      queryParams.set('cursor', params.cursor.toString());
    }
    if (params?.direction !== undefined) {
      queryParams.set('direction', params.direction);
    }
    const queryString = queryParams.toString();
    const url = queryString
      ? `/api/logs/${executionId}?${queryString}`
      : `/api/logs/${executionId}`;
    const response = await makeRequest(url);
    return handleApiResponse<PaginatedLogs>(response);
  },

  /**
   * Get the WebSocket URL for live log streaming.
   * Use this to subscribe to new log entries as they are produced.
   *
   * @param executionId - The execution process ID
   * @param token - Optional connection token for external access
   */
  getLiveStreamUrl: (executionId: string, token?: string): string => {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const host = window.location.host;
    const tokenParam = token ? `?token=${encodeURIComponent(token)}` : '';
    return `${protocol}//${host}/api/logs/${executionId}/live${tokenParam}`;
  },
};

// Terminal API
export interface CreateTerminalSessionRequest {
  working_dir: string;
}

export const terminalApi = {
  createSession: async (workingDir: string): Promise<CreateSessionResponse> => {
    const response = await makeRequest('/api/terminal/sessions', {
      method: 'POST',
      body: JSON.stringify({ working_dir: workingDir }),
    });
    return handleApiResponse<CreateSessionResponse>(response);
  },

  listSessions: async (): Promise<SessionInfo[]> => {
    const response = await makeRequest('/api/terminal/sessions');
    return handleApiResponse<SessionInfo[]>(response);
  },

  getSession: async (sessionId: string): Promise<SessionInfo> => {
    const response = await makeRequest(`/api/terminal/sessions/${sessionId}`);
    return handleApiResponse<SessionInfo>(response);
  },

  deleteSession: async (sessionId: string): Promise<void> => {
    const response = await makeRequest(`/api/terminal/sessions/${sessionId}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },

  getWebSocketUrl: (sessionId: string): string => {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const host = window.location.host;
    return `${protocol}//${host}/api/terminal/ws/${sessionId}`;
  },
};

// Message Queue API (in-memory queue for follow-up messages)
export const messageQueueApi = {
  list: async (attemptId: string): Promise<QueuedMessage[]> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/message-queue`
    );
    return handleApiResponse<QueuedMessage[]>(response);
  },

  add: async (
    attemptId: string,
    content: string,
    variant: string | null = null
  ): Promise<QueuedMessage> => {
    const payload: AddQueuedMessageRequest = { content, variant };
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/message-queue`,
      {
        method: 'POST',
        body: JSON.stringify(payload),
      }
    );
    return handleApiResponse<QueuedMessage>(response);
  },

  update: async (
    attemptId: string,
    messageId: string,
    content: string | null,
    variant: string | null = null
  ): Promise<QueuedMessage> => {
    const payload: UpdateQueuedMessageRequest = { content, variant };
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/message-queue/${messageId}`,
      {
        method: 'PUT',
        body: JSON.stringify(payload),
      }
    );
    return handleApiResponse<QueuedMessage>(response);
  },

  remove: async (attemptId: string, messageId: string): Promise<void> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/message-queue/${messageId}`,
      {
        method: 'DELETE',
      }
    );
    return handleApiResponse<void>(response);
  },

  reorder: async (
    attemptId: string,
    messageIds: string[]
  ): Promise<QueuedMessage[]> => {
    const payload: ReorderQueuedMessagesRequest = { message_ids: messageIds };
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/message-queue/reorder`,
      {
        method: 'POST',
        body: JSON.stringify(payload),
      }
    );
    return handleApiResponse<QueuedMessage[]>(response);
  },

  clear: async (attemptId: string): Promise<void> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/message-queue`,
      {
        method: 'DELETE',
      }
    );
    return handleApiResponse<void>(response);
  },
};
