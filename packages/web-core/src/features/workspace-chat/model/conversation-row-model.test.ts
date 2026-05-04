import { describe, it, expect } from 'vitest';
import {
  findPreviousUserMessageIndex,
  findNextUserMessageIndex,
  type ConversationRow,
} from './conversation-row-model';

function row(isUserMessage: boolean): ConversationRow {
  return { isUserMessage } as unknown as ConversationRow;
}

describe('findPreviousUserMessageIndex', () => {
  it('returns -1 for empty rows', () => {
    expect(findPreviousUserMessageIndex([], 0)).toBe(-1);
  });

  it('returns -1 when no earlier user message exists', () => {
    const rows = [row(false), row(false), row(true)];
    expect(findPreviousUserMessageIndex(rows, 0)).toBe(-1);
    expect(findPreviousUserMessageIndex(rows, 1)).toBe(-1);
  });

  it('returns the nearest earlier user message index, exclusive of beforeIndex', () => {
    const rows = [row(true), row(false), row(true), row(false)];
    expect(findPreviousUserMessageIndex(rows, 3)).toBe(2);
    expect(findPreviousUserMessageIndex(rows, 2)).toBe(0);
  });
});

describe('findNextUserMessageIndex', () => {
  it('returns -1 for empty rows', () => {
    expect(findNextUserMessageIndex([], -1)).toBe(-1);
  });

  it('returns -1 when no later user message exists', () => {
    const rows = [row(true), row(false), row(false)];
    expect(findNextUserMessageIndex(rows, 0)).toBe(-1);
    expect(findNextUserMessageIndex(rows, 2)).toBe(-1);
  });

  it('returns the nearest later user message index, exclusive of afterIndex', () => {
    const rows = [row(true), row(false), row(true), row(false), row(true)];
    expect(findNextUserMessageIndex(rows, 0)).toBe(2);
    expect(findNextUserMessageIndex(rows, 2)).toBe(4);
    expect(findNextUserMessageIndex(rows, -1)).toBe(0);
  });

  it('is symmetric with findPreviousUserMessageIndex', () => {
    const rows = [row(true), row(false), row(true), row(false), row(true)];
    expect(findPreviousUserMessageIndex(rows, 4)).toBe(2);
    expect(findNextUserMessageIndex(rows, 2)).toBe(4);
  });
});
