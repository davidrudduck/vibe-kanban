import { makeLocalApiRequest } from '@/shared/lib/localApiTransport';
import type { DiagnosticsResponse, DiskUsageResponse } from 'shared/types';

export async function getDiagnostics(): Promise<DiagnosticsResponse> {
  const response = await makeLocalApiRequest('/api/diagnostics');
  if (!response.ok) {
    throw new Error(`Failed to fetch diagnostics: ${response.status}`);
  }
  return response.json();
}

export async function getDiskUsage(): Promise<DiskUsageResponse> {
  const response = await makeLocalApiRequest('/api/diagnostics/disk-usage');
  if (!response.ok) {
    throw new Error(`Failed to fetch disk usage: ${response.status}`);
  }
  return response.json();
}
