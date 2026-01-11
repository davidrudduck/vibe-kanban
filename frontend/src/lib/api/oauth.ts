/**
 * OAuth API - Authentication flow handling.
 */

import { StatusResponse } from 'shared/types';

import { makeRequest, handleApiResponse, ApiError } from './utils';

/**
 * OAuth API namespace - Authentication flow handling.
 */
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
