import { useCallback } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useJsonPatchWsStream } from './useJsonPatchWsStream';
import type { ExecutionProcess } from 'shared/types';
import { makeRequest, handleApiResponse } from '@/lib/api/utils';

type ExecutionProcessState = {
  execution_processes: Record<string, ExecutionProcess>;
};

interface UseExecutionProcessesResult {
  executionProcesses: ExecutionProcess[];
  executionProcessesById: Record<string, ExecutionProcess>;
  isAttemptRunning: boolean;
  isLoading: boolean;
  isConnected: boolean;
  error: string | null;
}

interface NodeExecutionProcess {
  id: string;
  attempt_id: string;
  node_id: string;
  run_reason: string;
  executor_action: unknown;
  before_head_commit: string | null;
  after_head_commit: string | null;
  status: string;
  exit_code: number | null;
  dropped: boolean;
  pid: number | null;
  started_at: string;
  completed_at: string | null;
  created_at: string;
}

interface HiveAttemptResponse {
  attempt: unknown;
  executions: NodeExecutionProcess[];
  is_complete: boolean;
  node_info?: unknown;
}

/**
 * Stream execution processes for a task attempt via WebSocket (JSON Patch) and expose as array + map.
 * Server sends initial snapshot: replace /execution_processes with an object keyed by id.
 * Live updates arrive at /execution_processes/<id> via add/replace/remove operations.
 *
 * For remote attempts (assignmentId provided), fetches from Hive API instead of local WebSocket.
 */
export const useExecutionProcesses = (
  taskAttemptId: string | undefined,
  opts?: { showSoftDeleted?: boolean; assignmentId?: string }
): UseExecutionProcessesResult => {
  const showSoftDeleted = opts?.showSoftDeleted;
  const assignmentId = opts?.assignmentId;

  // Remote attempt: fetch from Hive API
  const {
    data: hiveData,
    isLoading: hiveLoading,
    error: hiveError,
  } = useQuery({
    queryKey: ['hive-attempt-executions', assignmentId],
    queryFn: async () => {
      if (!assignmentId) return null;
      const response = await makeRequest(
        `/api/database/hive/attempts/${assignmentId}`
      );
      return handleApiResponse<HiveAttemptResponse>(response);
    },
    enabled: !!assignmentId,
    refetchInterval: 5000, // Poll every 5 seconds for updates
  });

  // Local attempt: stream via WebSocket
  let endpoint: string | undefined;
  if (taskAttemptId && !assignmentId) {
    const params = new URLSearchParams({ task_attempt_id: taskAttemptId });
    if (typeof showSoftDeleted === 'boolean') {
      params.set('show_soft_deleted', String(showSoftDeleted));
    }
    endpoint = `/api/execution-processes/stream/ws?${params.toString()}`;
  }

  const initialData = useCallback(
    (): ExecutionProcessState => ({ execution_processes: {} }),
    []
  );

  const { data: wsData, isConnected, error: wsError } =
    useJsonPatchWsStream<ExecutionProcessState>(
      endpoint,
      !!taskAttemptId && !assignmentId,
      initialData
    );

  // Use Hive data if available, otherwise WebSocket data
  let executionProcessesById: Record<string, ExecutionProcess>;
  let isLoading: boolean;
  let error: string | null;

  if (assignmentId) {
    // Map NodeExecutionProcess to ExecutionProcess format
    executionProcessesById = (hiveData?.executions ?? []).reduce(
      (acc, exec) => {
        acc[exec.id] = {
          id: exec.id,
          task_attempt_id: exec.attempt_id,
          run_reason: exec.run_reason as ExecutionProcess['run_reason'],
          executor_action: exec.executor_action as ExecutionProcess['executor_action'],
          before_head_commit: exec.before_head_commit,
          after_head_commit: exec.after_head_commit,
          status: exec.status as ExecutionProcess['status'],
          exit_code: exec.exit_code !== null ? BigInt(exec.exit_code) : null,
          dropped: exec.dropped,
          pid: exec.pid !== null ? BigInt(exec.pid) : null,
          started_at: exec.started_at,
          completed_at: exec.completed_at,
          created_at: exec.created_at,
          updated_at: exec.created_at, // Use created_at as fallback for updated_at
          completion_reason: undefined,
          completion_message: undefined,
        };
        return acc;
      },
      {} as Record<string, ExecutionProcess>
    );
    isLoading = hiveLoading;
    error = hiveError ? String(hiveError) : null;
  } else {
    executionProcessesById = wsData?.execution_processes ?? {};
    isLoading = !!taskAttemptId && !wsData && !wsError;
    error = wsError;
  }

  const executionProcesses = Object.values(executionProcessesById).sort(
    (a, b) =>
      new Date(a.created_at as unknown as string).getTime() -
      new Date(b.created_at as unknown as string).getTime()
  );

  const isAttemptRunning = executionProcesses.some(
    (process) =>
      (process.run_reason === 'codingagent' ||
        process.run_reason === 'setupscript' ||
        process.run_reason === 'cleanupscript') &&
      process.status === 'running'
  );

  return {
    executionProcesses,
    executionProcessesById,
    isAttemptRunning,
    isLoading,
    isConnected: assignmentId ? !hiveError : isConnected,
    error,
  };
};
