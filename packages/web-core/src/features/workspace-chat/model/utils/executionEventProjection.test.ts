import { describe, expect, it } from 'vitest';
import type { ExecutionLogEvent, PatchType } from 'shared/types';

import { projectExecutionEventsToConversation } from './executionEventProjection';
import {
  getRunningAppendOnlyConversationResult,
  mergeAppendOnlyConversationItems,
  shouldFollowConversationTail,
} from './appendOnlyConversation';
import type { PatchTypeWithKey } from '@/shared/hooks/useConversationHistory/types';

const event = (
  id: number,
  event_type: ExecutionLogEvent['event_type'],
  payload_json: ExecutionLogEvent['payload_json'],
  execution_id = 'process-1'
): ExecutionLogEvent => ({
  id: BigInt(id),
  execution_id,
  source: 'test',
  source_event_id: `event-${id}`,
  event_type,
  payload_json,
  created_at: `2026-05-13T00:00:${String(id).padStart(2, '0')}Z`,
});

const stdoutItem = (
  patchKey: string,
  content: string,
  executionProcessId = 'process-1'
): PatchTypeWithKey => ({
  type: 'STDOUT',
  content,
  patchKey,
  executionProcessId,
});

const normalizedMessagePatch = (content: string): PatchType => ({
  type: 'NORMALIZED_ENTRY',
  content: {
    entry_type: { type: 'assistant_message' },
    content,
    timestamp: null,
  },
});

describe('projectExecutionEventsToConversation', () => {
  it('deduplicates by durable event id and sorts by cursor order', () => {
    const projected = projectExecutionEventsToConversation([
      event(2, 'raw_stdout', { text: 'two' }),
      event(1, 'raw_stdout', { text: 'one' }),
      event(2, 'raw_stdout', { text: 'two duplicate' }),
    ]);

    expect(projected.entries).toEqual([
      stdoutItem('process-1:event:1', 'one'),
      stdoutItem('process-1:event:2', 'two duplicate'),
    ]);
  });

  it('projects JSON patch add operations into stable conversation entries', () => {
    const projected = projectExecutionEventsToConversation([
      event(10, 'json_patch', [
        {
          op: 'add',
          path: '/entries/-',
          value: normalizedMessagePatch('hello'),
        },
      ]),
    ]);

    expect(projected.entries).toEqual([
      {
        ...normalizedMessagePatch('hello'),
        patchKey: 'process-1:event:10:0',
        executionProcessId: 'process-1',
      },
    ]);
  });

  it('ignores terminal reset events without wiping the transcript', () => {
    const projected = projectExecutionEventsToConversation([
      event(1, 'raw_stdout', { text: 'before reset' }),
      event(2, 'reset_ignored', { reason: 'screen clear' }),
      event(3, 'raw_stdout', { text: 'after reset' }),
    ]);

    expect(projected.ignoredResetCount).toBe(1);
    expect(projected.entries).toEqual([
      stdoutItem('process-1:event:1', 'before reset'),
      stdoutItem('process-1:event:3', 'after reset'),
    ]);
  });
});

describe('appendOnlyConversation', () => {
  it('keeps older rows when a partial live update omits them', () => {
    expect(
      mergeAppendOnlyConversationItems(
        [
          stdoutItem('process-1:event:1', 'one'),
          stdoutItem('process-1:event:2', 'two'),
        ],
        [stdoutItem('process-1:event:2', 'two updated')]
      )
    ).toEqual([
      stdoutItem('process-1:event:1', 'one'),
      stdoutItem('process-1:event:2', 'two updated'),
    ]);
  });

  it('rejects shorter stale replays without losing current rows', () => {
    const previous = [
      stdoutItem('process-1:event:1', 'one'),
      stdoutItem('process-1:event:2', 'two'),
    ];

    expect(
      getRunningAppendOnlyConversationResult(
        previous,
        [stdoutItem('process-1:event:1', 'one')],
        previous
      )
    ).toEqual({
      acceptedSnapshot: false,
      items: previous,
    });
  });

  it('inserts out-of-order cursor rows before their known anchor', () => {
    expect(
      mergeAppendOnlyConversationItems(
        [
          stdoutItem('process-1:event:2', 'two'),
          stdoutItem('process-1:event:3', 'three'),
        ],
        [
          stdoutItem('process-1:event:1', 'one'),
          stdoutItem('process-1:event:3', 'three updated'),
        ]
      )
    ).toEqual([
      stdoutItem('process-1:event:1', 'one'),
      stdoutItem('process-1:event:2', 'two'),
      stdoutItem('process-1:event:3', 'three updated'),
    ]);
  });

  it('does not auto-follow when the user is reading earlier rows', () => {
    expect(
      shouldFollowConversationTail({
        wasAtBottom: false,
        previousItems: [stdoutItem('process-1:event:1', 'one')],
        nextItems: [
          stdoutItem('process-1:event:1', 'one'),
          stdoutItem('process-1:event:2', 'two'),
        ],
      })
    ).toBe(false);
  });

  it('auto-follows while at bottom when the tail grows or changes', () => {
    expect(
      shouldFollowConversationTail({
        wasAtBottom: true,
        previousItems: [stdoutItem('process-1:event:1', 'one')],
        nextItems: [
          stdoutItem('process-1:event:1', 'one'),
          stdoutItem('process-1:event:2', 'two'),
        ],
      })
    ).toBe(true);
  });
});
