import { useCallback, useMemo } from 'react';
import type { Diff, PatchType } from 'shared/types';
import { useJsonPatchWsStream } from './useJsonPatchWsStream';

interface DiffEntries {
  [filePath: string]: PatchType;
}

type DiffStreamEvent = {
  entries: DiffEntries;
};

export interface UseDiffStreamOptions {
  statsOnly?: boolean;
}

interface UseDiffStreamResult {
  diffs: Diff[];
  error: string | null;
}

export const useDiffStream = (
  attemptId: string | null,
  enabled: boolean,
  options?: UseDiffStreamOptions
): UseDiffStreamResult => {
  // Memoize endpoint to prevent unnecessary WebSocket reconnections
  // Without useMemo, the endpoint string is recreated on every render,
  // causing useJsonPatchWsStream to see a "new" endpoint and reconnect
  const statsOnly = options?.statsOnly;
  const endpoint = useMemo(() => {
    if (!attemptId) return undefined;
    const query = `/api/task-attempts/${attemptId}/diff/ws`;
    if (typeof statsOnly === 'boolean') {
      const params = new URLSearchParams();
      params.set('stats_only', String(statsOnly));
      return `${query}?${params.toString()}`;
    } else {
      return query;
    }
  }, [attemptId, statsOnly]);

  const initialData = useCallback(
    (): DiffStreamEvent => ({
      entries: {},
    }),
    []
  );

  const { data, error } = useJsonPatchWsStream<DiffStreamEvent>(
    endpoint,
    enabled && !!attemptId,
    initialData
    // No need for injectInitialEntry or deduplicatePatches for diffs
  );

  const diffs = useMemo(() => {
    return Object.values(data?.entries ?? {})
      .filter((entry) => entry?.type === 'DIFF')
      .map((entry) => entry.content);
  }, [data?.entries]);

  return { diffs, error };
};
