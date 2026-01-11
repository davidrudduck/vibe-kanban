/**
 * API module - Re-exports from all API submodules.
 *
 * This file will be populated as endpoints are migrated from lib/api.ts.
 */

// Utils
export {
  ApiError,
  REQUEST_TIMEOUT_MS,
  anySignal,
  makeRequest,
  handleApiResponse,
  handleApiResponseAsResult,
} from './utils';

export type { Ok, Err, Result } from './utils';

// Projects API
export { projectsApi } from './projects';

// Tasks API
export { tasksApi } from './tasks';
export type {
  ProjectNodeInfo,
  ListProjectNodesResponse,
  TaskStreamConnectionInfoResponse,
} from './tasks';

// Task Attempts API
export { attemptsApi } from './attempts';

// Task Variables API
export { taskVariablesApi } from './taskVariables';

// Commits API
export { commitsApi } from './commits';

// Health API
export { healthApi } from './health';
export type { HealthResponse } from './health';

// Config API
export { configApi } from './config';

// Filesystem APIs
export { fileSystemApi, fileBrowserApi } from './filesystem';

// Execution Processes API
export { executionProcessesApi } from './execution';

// Templates API
export { templatesApi } from './templates';

// Labels API
export { labelsApi } from './labels';

// MCP Servers API
export { mcpServersApi } from './mcp';

// Profiles API
export { profilesApi } from './profiles';

// Images API
export { imagesApi } from './images';
