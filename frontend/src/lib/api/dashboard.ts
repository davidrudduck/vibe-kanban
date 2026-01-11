/**
 * Dashboard API namespace.
 */

import type { ActivityFeed, DashboardSummary } from 'shared/types';

import { handleApiResponse, makeRequest } from './utils';

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
