import { makeLocalApiRequest } from '@/shared/lib/localApiTransport';
import { handleApiResponse } from '@/shared/lib/api';
import type {
  CleanArtifactsResult,
  DiagnosticsResponse,
  DiskUsageResponse,
  RemoveWorktreeResult,
} from 'shared/types';

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

export async function cleanArtifacts(
  workspaceId: string
): Promise<CleanArtifactsResult> {
  const response = await makeLocalApiRequest(
    `/api/diagnostics/disk-usage/${workspaceId}/clean-artifacts`,
    { method: 'POST' }
  );
  if (!response.ok) {
    throw new Error(`Failed to clean artifacts: ${response.status}`);
  }
  return handleApiResponse<CleanArtifactsResult>(response);
}

export async function removeWorktree(
  workspaceId: string
): Promise<RemoveWorktreeResult> {
  const response = await makeLocalApiRequest(
    `/api/diagnostics/disk-usage/${workspaceId}/remove-worktree`,
    { method: 'POST' }
  );
  if (!response.ok) {
    throw new Error(`Failed to remove worktree: ${response.status}`);
  }
  return handleApiResponse<RemoveWorktreeResult>(response);
}
