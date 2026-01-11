/**
 * Logs API namespace - Unified log access endpoints.
 */

import type { PaginatedLogs, Direction } from 'shared/types';
import { makeRequest, handleApiResponse } from './utils';

/**
 * Parameters for paginated log queries.
 */
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
