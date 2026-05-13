import type { PaginatedExecutionLogEvents } from 'shared/types';
import { handleApiResponse } from '@/shared/lib/api';
import { makeLocalApiRequest } from '@/shared/lib/localApiTransport';

const toQuery = (params: Record<string, number | undefined>) => {
  const query = new URLSearchParams();
  Object.entries(params).forEach(([key, value]) => {
    if (value != null) query.set(key, String(value));
  });
  const value = query.toString();
  return value ? `?${value}` : '';
};

export const fetchExecutionLogEvents = async (
  executionProcessId: string,
  params: {
    afterId?: number;
    beforeId?: number;
    limit?: number;
  } = {}
): Promise<PaginatedExecutionLogEvents> => {
  const response = await makeLocalApiRequest(
    `/api/execution-processes/${executionProcessId}/events${toQuery({
      after_id: params.afterId,
      before_id: params.beforeId,
      limit: params.limit,
    })}`
  );
  return handleApiResponse<PaginatedExecutionLogEvents>(response);
};

export const fetchLatestExecutionLogEvents = async (
  executionProcessId: string,
  limit = 100
): Promise<PaginatedExecutionLogEvents> => {
  const response = await makeLocalApiRequest(
    `/api/execution-processes/${executionProcessId}/events/latest${toQuery({
      limit,
    })}`
  );
  return handleApiResponse<PaginatedExecutionLogEvents>(response);
};
