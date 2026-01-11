/**
 * Diagnostics API namespace - System diagnostic endpoints.
 */

import type { DiskUsageStats } from 'shared/types';
import { makeRequest, handleApiResponse } from './utils';

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
