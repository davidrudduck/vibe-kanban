/**
 * Commits API namespace - operations for commit information within task attempts.
 */

import type { CommitInfo, CommitCompareResult } from 'shared/types';
import { makeRequest, handleApiResponse } from './utils';

// Commits APIs
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
