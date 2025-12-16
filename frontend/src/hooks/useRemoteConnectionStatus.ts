import { useState, useEffect, useMemo } from 'react';
import { tasksApi, TaskStreamConnectionInfoResponse } from '@/lib/api';
import type { ConnectionStatus } from '@/components/common/ConnectionStatusBadge';
import type { TaskWithAttemptStatus } from 'shared/types';

interface UseRemoteConnectionStatusOptions {
  /** Whether to enable fetching connection info */
  enabled?: boolean;
}

interface UseRemoteConnectionStatusResult {
  /** The connection status: local, direct, relay, or disconnected */
  status: ConnectionStatus;
  /** Raw connection info from the server (null for local tasks) */
  connectionInfo: TaskStreamConnectionInfoResponse | null;
  /** Whether connection info is being fetched */
  isLoading: boolean;
  /** Error message if fetching failed */
  error: string | null;
}

/**
 * Hook to determine the connection status for a task.
 * For local tasks, returns 'local'.
 * For remote tasks, fetches connection info and returns 'direct', 'relay', or 'disconnected'.
 */
export function useRemoteConnectionStatus(
  task: TaskWithAttemptStatus | null | undefined,
  options?: UseRemoteConnectionStatusOptions
): UseRemoteConnectionStatusResult {
  const { enabled = true } = options ?? {};

  const isRemote = Boolean(task?.is_remote && task?.shared_task_id);
  const taskId = task?.id;

  const [connectionInfo, setConnectionInfo] = useState<TaskStreamConnectionInfoResponse | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Fetch connection info for remote tasks
  useEffect(() => {
    if (!isRemote || !taskId || !enabled) {
      setConnectionInfo(null);
      setIsLoading(false);
      setError(null);
      return;
    }

    let cancelled = false;
    setIsLoading(true);

    const fetchInfo = async () => {
      try {
        const info = await tasksApi.streamConnectionInfo(taskId);
        if (!cancelled) {
          setConnectionInfo(info);
          setError(null);
        }
      } catch (e) {
        if (!cancelled) {
          console.error('Failed to fetch connection status:', e);
          setError(
            e instanceof Error ? e.message : 'Failed to determine connection status'
          );
          setConnectionInfo(null);
        }
      } finally {
        if (!cancelled) {
          setIsLoading(false);
        }
      }
    };

    void fetchInfo();

    return () => {
      cancelled = true;
    };
  }, [isRemote, taskId, enabled]);

  const status: ConnectionStatus = useMemo(() => {
    // Local task
    if (!isRemote) {
      return 'local';
    }

    // Remote task - check connection info
    if (error) {
      return 'disconnected';
    }

    if (!connectionInfo) {
      // Still loading or no info yet
      return isLoading ? 'local' : 'disconnected';
    }

    // Has connection info - determine type
    if (connectionInfo.direct_url) {
      return 'direct';
    }

    // No direct URL means relay (or future relay support)
    return 'relay';
  }, [isRemote, connectionInfo, error, isLoading]);

  return {
    status,
    connectionInfo,
    isLoading,
    error,
  };
}
