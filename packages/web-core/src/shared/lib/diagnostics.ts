import { makeLocalApiRequest } from '@/shared/lib/localApiTransport';
import { handleApiResponse } from '@/shared/lib/api';
import type { DiagnosticsResponse, DiskUsageResponse } from 'shared/types';

export async function getDiagnostics(): Promise<DiagnosticsResponse> {
  const response = await makeLocalApiRequest('/api/diagnostics');
  if (!response.ok) {
    throw new Error(`Failed to fetch diagnostics: ${response.status}`);
  }
  return handleApiResponse<DiagnosticsResponse>(response);
}

export async function getDiskUsage(): Promise<DiskUsageResponse> {
  const response = await makeLocalApiRequest('/api/diagnostics/disk-usage');
  if (!response.ok) {
    throw new Error(`Failed to fetch disk usage: ${response.status}`);
  }
  return handleApiResponse<DiskUsageResponse>(response);
}
