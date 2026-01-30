import { useCallback, useEffect, useMemo, useState } from 'react';
import type { Diff, PatchType } from 'shared/types';
import { useJsonPatchWsStream } from './useJsonPatchWsStream';
import { tasksApi, TaskStreamConnectionInfoResponse } from '@/lib/api';

interface DiffEntries {
  [filePath: string]: PatchType;
}

type DiffStreamEvent = {
  entries: DiffEntries;
};

export interface UseDiffStreamOptions {
  statsOnly?: boolean;
}

/**
 * Remote connection info passed to useDiffStream for cross-node streaming.
 * Contains the local task ID used to fetch connection info from the hive.
 */
export interface RemoteStreamInfo {
  /** Local task ID (which has shared_task_id) for fetching connection info */
  taskId: string;
}

interface UseDiffStreamResult {
  diffs: Diff[];
  error: string | null;
  /** Connection type for remote streams: 'direct', 'relay', or null for local */
  connectionType: 'direct' | 'relay' | null;
}

/**
 * Hook for streaming diff updates from a task attempt.
 *
 * For local attempts, connects directly to the local WebSocket endpoint.
 * For remote attempts (when remoteInfo is provided):
 * 1. Fetches connection info from the server (which proxies to hive)
 * 2. Tries direct connection to the remote node first
 * 3. Falls back to hive relay if direct connection fails
 *
 * @param attemptId - The task attempt ID (local or remote)
 * @param enabled - Whether streaming is enabled
 * @param options - Additional options like statsOnly
 * @param remoteInfo - If provided, stream from a remote node instead of local
 */
export const useDiffStream = (
  attemptId: string | null,
  enabled: boolean,
  options?: UseDiffStreamOptions,
  remoteInfo?: RemoteStreamInfo
): UseDiffStreamResult => {
  const statsOnly = options?.statsOnly;
  const [connectionInfo, setConnectionInfo] =
    useState<TaskStreamConnectionInfoResponse | null>(null);
  const [connectionType, setConnectionType] = useState<
    'direct' | 'relay' | null
  >(null);
  const [fetchError, setFetchError] = useState<string | null>(null);
  const [connectionError, setConnectionError] = useState<string | null>(null);

  // Fetch connection info for remote streams
  useEffect(() => {
    if (!remoteInfo || !enabled) {
      setConnectionInfo(null);
      setConnectionType(null);
      setFetchError(null);
      return;
    }

    let cancelled = false;

    const fetchInfo = async () => {
      try {
        const info = await tasksApi.streamConnectionInfo(remoteInfo.taskId);
        if (!cancelled) {
          setConnectionInfo(info);
          setFetchError(null);
          // Determine connection type based on whether direct_url is available
          setConnectionType(info.direct_url ? 'direct' : 'relay');
        }
      } catch (e) {
        if (!cancelled) {
          console.error('Failed to fetch stream connection info:', e);
          setFetchError(
            e instanceof Error
              ? e.message
              : 'Failed to fetch stream connection info'
          );
          setConnectionInfo(null);
          setConnectionType(null);
        }
      }
    };

    void fetchInfo();

    return () => {
      cancelled = true;
    };
  }, [remoteInfo?.taskId, enabled, remoteInfo]);

  // Build endpoint URL
  // For local: /api/task-attempts/{attemptId}/diff/ws
  // For remote direct: wss://{node}/api/task-attempts/{attempt_id}/diff/ws?token=...
  // For remote relay: wss://{hive}/v1/tasks/{taskId}/diff/relay?token=... (future)
  const endpoint = useMemo(() => {
    // Remote stream - need connection info first
    if (remoteInfo) {
      if (!connectionInfo) return undefined;

      // Get the attempt ID from connection info (from hive assignment)
      const remoteAttemptId = connectionInfo.attempt_id;
      if (!remoteAttemptId) {
        console.warn('Remote stream connection info missing attempt_id');
        return undefined;
      }

      // Build query params
      const params = new URLSearchParams();
      params.set('token', connectionInfo.connection_token);
      if (typeof statsOnly === 'boolean') {
        params.set('stats_only', String(statsOnly));
      }

      // Try direct connection first if available
      if (connectionInfo.direct_url) {
        try {
          const directUrl = new URL(connectionInfo.direct_url);
          const wsProtocol = directUrl.protocol === 'https:' ? 'wss:' : 'ws:';
          return `${wsProtocol}//${directUrl.host}/api/task-attempts/${remoteAttemptId}/diff/ws?${params.toString()}`;
        } catch {
          // Invalid URL, fall through to relay
          console.error(
            'Invalid direct_url for remote stream:',
            connectionInfo.direct_url
          );
        }
      }

      // Try relay URL if direct connection not available
      if (connectionInfo.relay_url) {
        try {
          const relayUrl = new URL(connectionInfo.relay_url);
          const wsProtocol = relayUrl.protocol === 'https:' ? 'wss:' : 'ws:';
          // Relay endpoints use a different path structure
          return `${wsProtocol}//${relayUrl.host}${relayUrl.pathname}?${params.toString()}`;
        } catch {
          console.error(
            'Invalid relay_url for remote stream:',
            connectionInfo.relay_url
          );
        }
      }

      // No valid connection method available - this is a configuration issue
      // The error will be shown to the user via the error state
      return undefined;
    }

    // Local stream
    if (!attemptId) return undefined;
    const query = `/api/task-attempts/${attemptId}/diff/ws`;
    if (typeof statsOnly === 'boolean') {
      const params = new URLSearchParams();
      params.set('stats_only', String(statsOnly));
      return `${query}?${params.toString()}`;
    } else {
      return query;
    }
  }, [attemptId, statsOnly, remoteInfo, connectionInfo]);

  // Set connection error when we have remote info and connection info but no valid endpoint
  useEffect(() => {
    if (remoteInfo && connectionInfo && !endpoint) {
      setConnectionError(
        'Unable to connect to remote node. The node may be offline or unreachable.'
      );
    } else {
      setConnectionError(null);
    }
  }, [remoteInfo, connectionInfo, endpoint]);

  const initialData = useCallback(
    (): DiffStreamEvent => ({
      entries: {},
    }),
    []
  );

  // Only enable stream when we have an endpoint
  const streamEnabled = enabled && !!endpoint;

  const { data, error: wsError } = useJsonPatchWsStream<DiffStreamEvent>(
    endpoint,
    streamEnabled,
    initialData
    // No need for injectInitialEntry or deduplicatePatches for diffs
  );

  const diffs = useMemo(() => {
    return Object.values(data?.entries ?? {})
      .filter((entry) => entry?.type === 'DIFF')
      .map((entry) => entry.content);
  }, [data?.entries]);

  // Combine errors - fetch error takes precedence, then connection error, then ws error
  const error = fetchError || connectionError || wsError;

  return { diffs, error, connectionType: remoteInfo ? connectionType : null };
};
