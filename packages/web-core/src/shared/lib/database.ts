import { makeLocalApiRequest } from '@/shared/lib/localApiTransport';
import { handleApiResponse } from '@/shared/lib/api';
import type {
  DatabaseStats,
  VacuumResult,
  AnalyzeResult,
  ArchivedStatsResponse,
  ArchivedPurgeResult,
  LogStatsResponse,
  LogPurgeResult,
} from 'shared/types';

export async function getDatabaseStats(): Promise<DatabaseStats> {
  const response = await makeLocalApiRequest('/api/database/stats');
  if (!response.ok) {
    throw new Error(`Failed to fetch database stats: ${response.status}`);
  }
  return handleApiResponse<DatabaseStats>(response);
}

export async function runVacuum(): Promise<VacuumResult> {
  const response = await makeLocalApiRequest('/api/database/vacuum', {
    method: 'POST',
  });
  if (response.status === 429) {
    const data = await response.json().catch(() => ({}));
    throw Object.assign(new Error('Vacuum is on cooldown'), {
      status: 429,
      data,
    });
  }
  if (!response.ok) {
    throw new Error(`Failed to run VACUUM: ${response.status}`);
  }
  return handleApiResponse<VacuumResult>(response);
}

export async function runAnalyze(): Promise<AnalyzeResult> {
  const response = await makeLocalApiRequest('/api/database/analyze', {
    method: 'POST',
  });
  if (!response.ok) {
    throw new Error(`Failed to run ANALYZE: ${response.status}`);
  }
  return handleApiResponse<AnalyzeResult>(response);
}

export async function getArchivedStats(
  olderThanDays?: number
): Promise<ArchivedStatsResponse> {
  const params = new URLSearchParams();
  if (olderThanDays != null) {
    params.set('older_than_days', String(olderThanDays));
  }
  const query = params.size > 0 ? `?${params.toString()}` : '';
  const response = await makeLocalApiRequest(
    `/api/database/archived-stats${query}`
  );
  if (!response.ok) {
    throw new Error(`Failed to fetch archived stats: ${response.status}`);
  }
  return handleApiResponse<ArchivedStatsResponse>(response);
}

export async function purgeArchived(
  olderThanDays?: number
): Promise<ArchivedPurgeResult> {
  const params = new URLSearchParams();
  if (olderThanDays != null) {
    params.set('older_than_days', String(olderThanDays));
  }
  const query = params.size > 0 ? `?${params.toString()}` : '';
  const response = await makeLocalApiRequest(
    `/api/database/purge-archived${query}`,
    { method: 'POST' }
  );
  if (!response.ok) {
    throw new Error(`Failed to purge archived workspaces: ${response.status}`);
  }
  return handleApiResponse<ArchivedPurgeResult>(response);
}

export async function getLogStats(
  olderThanDays?: number
): Promise<LogStatsResponse> {
  const params = new URLSearchParams();
  if (olderThanDays != null) {
    params.set('older_than_days', String(olderThanDays));
  }
  const query = params.size > 0 ? `?${params.toString()}` : '';
  const response = await makeLocalApiRequest(
    `/api/database/log-stats${query}`
  );
  if (!response.ok) {
    throw new Error(`Failed to fetch log stats: ${response.status}`);
  }
  return handleApiResponse<LogStatsResponse>(response);
}

export async function purgeLogs(
  olderThanDays?: number
): Promise<LogPurgeResult> {
  const params = new URLSearchParams();
  if (olderThanDays != null) {
    params.set('older_than_days', String(olderThanDays));
  }
  const query = params.size > 0 ? `?${params.toString()}` : '';
  const response = await makeLocalApiRequest(
    `/api/database/purge-logs${query}`,
    { method: 'POST' }
  );
  if (!response.ok) {
    throw new Error(`Failed to purge logs: ${response.status}`);
  }
  return handleApiResponse<LogPurgeResult>(response);
}
