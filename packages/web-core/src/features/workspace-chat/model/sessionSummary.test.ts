import { describe, it, expect } from 'vitest';
import {
  formatTokenCount,
  formatMsDuration,
  formatExecutorName,
  aggregateSessionSummary,
} from './sessionSummary';
import { BaseCodingAgent } from 'shared/types';
import type { TokenUsageInfo } from 'shared/types';

describe('sessionSummary display helpers', () => {
  describe('formatTokenCount', () => {
    it('formats null as "—"', () => {
      expect(formatTokenCount(null)).toBe('—');
    });

    it('formats small numbers as-is', () => {
      expect(formatTokenCount(0)).toBe('0');
      expect(formatTokenCount(72)).toBe('72');
      expect(formatTokenCount(999)).toBe('999');
    });

    it('formats thousands with one decimal', () => {
      expect(formatTokenCount(1_000)).toBe('1.0k');
      expect(formatTokenCount(1_500)).toBe('1.5k');
      expect(formatTokenCount(12_345)).toBe('12.3k');
      expect(formatTokenCount(999_999)).toBe('1000.0k');
    });

    it('formats millions with two decimals', () => {
      expect(formatTokenCount(1_000_000)).toBe('1.00M');
      expect(formatTokenCount(2_060_000)).toBe('2.06M');
      expect(formatTokenCount(15_500_000)).toBe('15.50M');
    });
  });

  describe('formatMsDuration', () => {
    it('formats null as "—"', () => {
      expect(formatMsDuration(null)).toBe('—');
    });

    it('formats seconds only for durations under 1 minute', () => {
      expect(formatMsDuration(0)).toBe('0s');
      expect(formatMsDuration(5_000)).toBe('5s');
      expect(formatMsDuration(45_000)).toBe('45s');
      expect(formatMsDuration(59_499)).toBe('59s');
    });

    it('formats minutes:seconds for longer durations', () => {
      expect(formatMsDuration(60_000)).toBe('1:00');
      expect(formatMsDuration(65_000)).toBe('1:05');
      expect(formatMsDuration(125_000)).toBe('2:05');
      expect(formatMsDuration(3_599_000)).toBe('59:59');
    });
  });

  describe('formatExecutorName', () => {
    it('formats null as "this executor"', () => {
      expect(formatExecutorName(null)).toBe('this executor');
    });

    it('uses display name map for known executors', () => {
      expect(formatExecutorName(BaseCodingAgent.CLAUDE_CODE)).toBe(
        'Claude Code SDK'
      );
      expect(formatExecutorName(BaseCodingAgent.CODEX)).toBe('Codex');
      expect(formatExecutorName(BaseCodingAgent.OPENCODE)).toBe('OpenCode');
      expect(formatExecutorName(BaseCodingAgent.CURSOR_AGENT)).toBe(
        'Cursor Agent'
      );
      expect(formatExecutorName(BaseCodingAgent.QWEN_CODE)).toBe('Qwen Code');
      expect(formatExecutorName(BaseCodingAgent.GEMINI)).toBe('Gemini');
      expect(formatExecutorName(BaseCodingAgent.AMP)).toBe('Amp');
      expect(formatExecutorName(BaseCodingAgent.COPILOT)).toBe('Copilot');
      expect(formatExecutorName(BaseCodingAgent.DROID)).toBe('Droid');
    });

    it('falls back to title case for unknown executors', () => {
      expect(formatExecutorName('FUTURE_EXECUTOR')).toBe('Future Executor');
      expect(formatExecutorName('NEW_AI_MODEL')).toBe('New Ai Model');
    });
  });

  describe('aggregateSessionSummary', () => {
    it('returns empty summary with executorSupportsTokens=false when no entries and unknown executor', () => {
      const summary = aggregateSessionSummary([], null);
      expect(summary.hasEntries).toBe(false);
      expect(summary.executorSupportsTokens).toBe(false);
      expect(summary.contextTokens).toBe(0);
    });

    it('returns empty summary with executorSupportsTokens=true when no entries but known emitter', () => {
      const summary = aggregateSessionSummary([], BaseCodingAgent.CLAUDE_CODE);
      expect(summary.hasEntries).toBe(false);
      expect(summary.executorSupportsTokens).toBe(true);
      expect(summary.contextTokens).toBe(0);
    });

    it('correctly accumulates cost as number (no precision loss)', () => {
      const entries: TokenUsageInfo[] = [
        {
          total_tokens: 1000n,
          model_context_window: 200000n,
          max_output_tokens: null,
          output_tokens: 100n,
          cache_creation_tokens: null,
          cache_read_tokens: null,
          cost_microusd: 50_000n, // $0.05
          duration_ms: 1000n,
          num_turns: null,
        },
        {
          total_tokens: 2000n,
          model_context_window: 200000n,
          max_output_tokens: null,
          output_tokens: 200n,
          cache_creation_tokens: null,
          cache_read_tokens: null,
          cost_microusd: 75_000n, // $0.075
          duration_ms: 2000n,
          num_turns: null,
        },
      ];

      const summary = aggregateSessionSummary(
        entries,
        BaseCodingAgent.CLAUDE_CODE
      );

      // Cost should be summed correctly: 0.05 + 0.075 = 0.125
      expect(summary.costUSD).toBe(0.125);
      expect(summary.outputTokens).toBe(300);
      expect(summary.durationMs).toBe(3000);
    });
  });
});
