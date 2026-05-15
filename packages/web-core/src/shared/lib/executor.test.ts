import { describe, expect, it } from 'vitest';
import { BaseCodingAgent } from 'shared/types';

import { sortExecutorsByDisplayName } from './executor';

describe('sortExecutorsByDisplayName', () => {
  it('sorts agents alphabetically by displayed name', () => {
    expect(
      sortExecutorsByDisplayName([
        BaseCodingAgent.OPENCODE,
        BaseCodingAgent.CLAUDE_TERMINAL,
        BaseCodingAgent.AMP,
        BaseCodingAgent.CODEX,
        BaseCodingAgent.COPILOT,
      ])
    ).toEqual([
      BaseCodingAgent.AMP,
      BaseCodingAgent.CLAUDE_TERMINAL,
      BaseCodingAgent.CODEX,
      BaseCodingAgent.COPILOT,
      BaseCodingAgent.OPENCODE,
    ]);
  });
});
