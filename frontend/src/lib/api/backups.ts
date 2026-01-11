/**
 * Backups API namespace - Database backup management endpoints.
 */

import type { BackupInfo, ApiResponse } from 'shared/types';
import { makeRequest, handleApiResponse, ApiError } from './utils';

export const backupsApi = {
  /**
   * List all available database backups, sorted newest first.
   */
  list: async (): Promise<BackupInfo[]> => {
    const response = await makeRequest('/api/backups');
    return handleApiResponse<BackupInfo[]>(response);
  },

  /**
   * Create a new database backup.
   */
  create: async (): Promise<BackupInfo> => {
    const response = await makeRequest('/api/backups', { method: 'POST' });
    return handleApiResponse<BackupInfo>(response);
  },

  /**
   * Delete a database backup by filename.
   */
  delete: async (filename: string): Promise<void> => {
    const response = await makeRequest(
      `/api/backups/${encodeURIComponent(filename)}`,
      {
        method: 'DELETE',
      }
    );
    return handleApiResponse<void>(response);
  },

  /**
   * Get the download URL for a backup file.
   * The browser will handle the download when navigated to this URL.
   */
  getDownloadUrl: (filename: string): string => {
    return `/api/backups/${encodeURIComponent(filename)}/download`;
  },

  /**
   * Restore database from an uploaded backup file.
   * Returns a message indicating the application needs to be restarted.
   */
  restore: async (file: File): Promise<string> => {
    const formData = new FormData();
    formData.append('backup', file);
    const response = await fetch('/api/backups/restore', {
      method: 'POST',
      body: formData,
    });
    const result: ApiResponse<string> = await response.json();
    if (!response.ok || !result.success) {
      throw new ApiError(
        result.message || 'Failed to restore backup',
        response.status,
        response
      );
    }
    return result.data!;
  },
};
