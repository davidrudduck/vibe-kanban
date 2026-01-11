/**
 * Terminal API namespace - Terminal session management endpoints.
 */

import type { SessionInfo, CreateSessionResponse } from 'shared/types';
import { makeRequest, handleApiResponse } from './utils';

/**
 * Request body for creating a terminal session.
 */
export interface CreateTerminalSessionRequest {
  working_dir: string;
}

export const terminalApi = {
  /**
   * Create a new terminal session in the specified working directory.
   *
   * @param workingDir - The working directory for the terminal session
   */
  createSession: async (workingDir: string): Promise<CreateSessionResponse> => {
    const response = await makeRequest('/api/terminal/sessions', {
      method: 'POST',
      body: JSON.stringify({ working_dir: workingDir }),
    });
    return handleApiResponse<CreateSessionResponse>(response);
  },

  /**
   * List all active terminal sessions.
   */
  listSessions: async (): Promise<SessionInfo[]> => {
    const response = await makeRequest('/api/terminal/sessions');
    return handleApiResponse<SessionInfo[]>(response);
  },

  /**
   * Get details for a specific terminal session.
   *
   * @param sessionId - The terminal session ID
   */
  getSession: async (sessionId: string): Promise<SessionInfo> => {
    const response = await makeRequest(`/api/terminal/sessions/${sessionId}`);
    return handleApiResponse<SessionInfo>(response);
  },

  /**
   * Delete/terminate a terminal session.
   *
   * @param sessionId - The terminal session ID to delete
   */
  deleteSession: async (sessionId: string): Promise<void> => {
    const response = await makeRequest(`/api/terminal/sessions/${sessionId}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },

  /**
   * Get the WebSocket URL for connecting to a terminal session.
   *
   * @param sessionId - The terminal session ID
   */
  getWebSocketUrl: (sessionId: string): string => {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const host = window.location.host;
    return `${protocol}//${host}/api/terminal/ws/${sessionId}`;
  },
};
