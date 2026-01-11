/**
 * Approvals API - Handle execution approvals.
 */

import { ApprovalStatus, ApprovalResponse } from 'shared/types';

import { makeRequest, handleApiResponse } from './utils';

/**
 * Approvals API namespace - Handle execution approvals.
 */
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
