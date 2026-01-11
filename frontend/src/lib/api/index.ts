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
