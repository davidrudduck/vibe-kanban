/**
 * Health check API namespace.
 */

import { makeRequest } from './utils';

/**
 * Health check response type.
 */
export interface HealthResponse {
  status: string;
  version: string;
  git_commit: string;
  git_branch: string;
  build_timestamp: string;
  database_ready: boolean;
}

/**
 * Health check API namespace.
 */
export const healthApi = {
  /**
   * Check the health status of the backend server.
   * Note: Health endpoint returns raw JSON, not wrapped in ApiResponse.
   */
  check: async (): Promise<HealthResponse> => {
    const response = await makeRequest('/api/health');
    // Health endpoint returns raw JSON, not wrapped in ApiResponse
    if (!response.ok) {
      throw new Error(`Health check failed: ${response.status}`);
    }
    return response.json();
  },
};
