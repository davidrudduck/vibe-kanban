import type {
  ExecutionLogEvent,
  JsonValue,
  NormalizedEntry,
  PatchType,
} from 'shared/types';
import type { PatchTypeWithKey } from '@/shared/hooks/useConversationHistory/types';

interface JsonPatchOperation {
  op?: string;
  path?: string;
  value?: JsonValue;
}

export interface ExecutionEventProjection {
  entries: PatchTypeWithKey[];
  ignoredResetCount: number;
}

const eventIdNumber = (event: ExecutionLogEvent): number =>
  Number(event.id as unknown as number | bigint);

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const getPayloadText = (payload: JsonValue): string => {
  if (typeof payload === 'string') return payload;
  if (isRecord(payload) && typeof payload.text === 'string') {
    return payload.text;
  }
  if (isRecord(payload) && typeof payload.content === 'string') {
    return payload.content;
  }
  return '';
};

const hasNormalizedEntryShape = (value: unknown): value is NormalizedEntry =>
  isRecord(value) &&
  isRecord(value.entry_type) &&
  typeof value.content === 'string';

const toPatch = (value: unknown): PatchType | null => {
  if (!isRecord(value) || typeof value.type !== 'string') return null;

  if (value.type === 'STDOUT' && typeof value.content === 'string') {
    return { type: 'STDOUT', content: value.content };
  }

  if (value.type === 'STDERR' && typeof value.content === 'string') {
    return { type: 'STDERR', content: value.content };
  }

  if (
    value.type === 'NORMALIZED_ENTRY' &&
    hasNormalizedEntryShape(value.content)
  ) {
    return {
      type: 'NORMALIZED_ENTRY',
      content: value.content,
    };
  }

  if (value.type === 'DIFF' && value.content != null) {
    return value as PatchType;
  }

  return null;
};

const extractJsonPatchPatches = (payload: JsonValue): PatchType[] => {
  if (!Array.isArray(payload)) return [];

  return payload.flatMap((operation) => {
    const op = operation as JsonPatchOperation;
    if (op.op !== 'add' && op.op !== 'replace') return [];
    if (!op.path?.startsWith('/entries/')) return [];

    const patch = toPatch(op.value);
    return patch ? [patch] : [];
  });
};

const patchWithEventKey = (
  event: ExecutionLogEvent,
  patch: PatchType,
  index?: number
): PatchTypeWithKey => {
  const id = eventIdNumber(event);
  const suffix = index == null ? `${id}` : `${id}:${index}`;
  return {
    ...patch,
    patchKey: `${event.execution_id}:event:${suffix}`,
    executionProcessId: event.execution_id,
  };
};

export const projectExecutionEventsToConversation = (
  events: ExecutionLogEvent[]
): ExecutionEventProjection => {
  let ignoredResetCount = 0;
  const eventsById = new Map<number, ExecutionLogEvent>();

  events.forEach((event) => {
    eventsById.set(eventIdNumber(event), event);
  });

  const entries = [...eventsById.values()]
    .sort((a, b) => eventIdNumber(a) - eventIdNumber(b))
    .flatMap((event) => {
      switch (event.event_type) {
        case 'raw_stdout':
          return [
            patchWithEventKey(event, {
              type: 'STDOUT',
              content: getPayloadText(event.payload_json),
            }),
          ];
        case 'raw_stderr':
          return [
            patchWithEventKey(event, {
              type: 'STDERR',
              content: getPayloadText(event.payload_json),
            }),
          ];
        case 'json_patch':
          return extractJsonPatchPatches(event.payload_json).map(
            (patch, index) => patchWithEventKey(event, patch, index)
          );
        case 'reset_ignored':
          ignoredResetCount += 1;
          return [];
        default:
          return [];
      }
    });

  return { entries, ignoredResetCount };
};
