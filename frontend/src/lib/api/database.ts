/**
 * Database API namespace - Database statistics and maintenance endpoints.
 */

import type {
  DatabaseStats,
  VacuumResult,
  ArchivedStatsResponse,
  ArchivedPurgeResult,
  LogStatsResponse,
  LogPurgeResult,
  Task,
} from 'shared/types';
import { makeRequest, handleApiResponse } from './utils';

export const databaseApi = {
  /**
   * Get database statistics including file sizes, page info, and table counts.
   */
  getStats: async (): Promise<DatabaseStats> => {
    const response = await makeRequest('/api/database/stats');
    return handleApiResponse<DatabaseStats>(response);
  },

  /**
   * Run VACUUM on the database to reclaim space from deleted records.
   * Returns information about the bytes freed by the operation.
   */
  vacuum: async (): Promise<VacuumResult> => {
    const response = await makeRequest('/api/database/vacuum', {
      method: 'POST',
    });
    return handleApiResponse<VacuumResult>(response);
  },

  /**
   * Run ANALYZE on the database to update query planner statistics.
   */
  analyze: async (): Promise<void> => {
    const response = await makeRequest('/api/database/analyze', {
      method: 'POST',
    });
    return handleApiResponse<void>(response);
  },

  /**
   * Get the count of archived tasks in terminal states (done/cancelled)
   * that are older than the specified number of days.
   *
   * @param olderThanDays - Number of days old a task must be (default: 14)
   */
  getArchivedStats: async (
    olderThanDays = 14
  ): Promise<ArchivedStatsResponse> => {
    const params = new URLSearchParams({
      older_than_days: String(olderThanDays),
    });
    const response = await makeRequest(
      `/api/database/archived-stats?${params}`
    );
    return handleApiResponse<ArchivedStatsResponse>(response);
  },

  /**
   * Get a list of archived tasks that are NOT in terminal states (done/cancelled).
   * These are "stuck" tasks that were archived but not completed.
   */
  getArchivedNonTerminal: async (): Promise<Task[]> => {
    const response = await makeRequest('/api/database/archived-non-terminal');
    return handleApiResponse<Task[]>(response);
  },

  /**
   * Delete archived tasks in terminal states (done/cancelled) that are older
   * than the specified number of days.
   *
   * @param olderThanDays - Number of days old a task must be (default: 14)
   * @returns The number of tasks deleted and the cutoff used
   */
  purgeArchived: async (olderThanDays = 14): Promise<ArchivedPurgeResult> => {
    const params = new URLSearchParams({
      older_than_days: String(olderThanDays),
    });
    const response = await makeRequest(
      `/api/database/purge-archived?${params}`,
      {
        method: 'POST',
      }
    );
    return handleApiResponse<ArchivedPurgeResult>(response);
  },

  /**
   * Get the count of log entries older than the specified number of days.
   *
   * @param olderThanDays - Number of days old a log entry must be (default: 14)
   */
  getLogStats: async (olderThanDays = 14): Promise<LogStatsResponse> => {
    const params = new URLSearchParams({
      older_than_days: String(olderThanDays),
    });
    const response = await makeRequest(`/api/database/log-stats?${params}`);
    return handleApiResponse<LogStatsResponse>(response);
  },

  /**
   * Delete log entries older than the specified number of days.
   *
   * @param olderThanDays - Number of days old a log entry must be (default: 14)
   * @returns The number of log entries deleted and the cutoff used
   */
  purgeLogs: async (olderThanDays = 14): Promise<LogPurgeResult> => {
    const params = new URLSearchParams({
      older_than_days: String(olderThanDays),
    });
    const response = await makeRequest(`/api/database/purge-logs?${params}`, {
      method: 'POST',
    });
    return handleApiResponse<LogPurgeResult>(response);
  },
};
