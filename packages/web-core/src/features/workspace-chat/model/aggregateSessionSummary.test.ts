import { describe, it, expect } from 'vitest';
import { aggregateSessionSummary } from './sessionSummary';
import type { TokenUsageInfo } from 'shared/types';

describe('aggregateSessionSummary', () => {
  it('returns empty summary when no processes', () => {
    const summary = aggregateSessionSummary([]);
    expect(summary.executorSupportsTokens).toBe(false);
    expect(summary.contextTokens).toBe(0);
    expect(summary.costUSD).toBeNull();
    expect(summary.cacheHitRate).toBeNull();
  });

  it('sums cost/duration across processes; uses latest for turns and context', () => {
    const process1: TokenUsageInfo = {
      total_tokens: 1000n,
      model_context_window: 200000n,
      cost_microusd: 500000n, // $0.50
      num_turns: 10,
      duration_ms: 60000n,
      output_tokens: 200n,
      cache_creation_tokens: null,
      cache_read_tokens: null,
      max_output_tokens: null,
    };
    const process2: TokenUsageInfo = {
      total_tokens: 2000n,
      model_context_window: 200000n,
      cost_microusd: 300000n, // $0.30
      // Claude's num_turns already counts all turns in the conversation,
      // including resumed turns — latest value wins (not a sum).
      num_turns: 15,
      duration_ms: 30000n,
      output_tokens: 100n,
      cache_creation_tokens: null,
      cache_read_tokens: null,
      max_output_tokens: null,
    };
    const summary = aggregateSessionSummary([process1, process2]);
    // Cost and duration are summed (additive across processes)
    expect(summary.costUSD).toBeCloseTo(0.8);
    expect(summary.durationMs).toBe(90000);
    // num_turns uses latest process only (not cumulative sum)
    expect(summary.numTurns).toBe(15);
    // context fields come from latest process
    expect(summary.contextTokens).toBe(2000);
    expect(summary.contextWindow).toBe(200000);
    // output tokens are summed
    expect(summary.outputTokens).toBe(300);
  });

  it('returns null cacheHitRate when denominator is zero', () => {
    const process: TokenUsageInfo = {
      total_tokens: 0n,
      model_context_window: 200000n,
      cache_creation_tokens: null,
      cache_read_tokens: null,
      output_tokens: null,
      cost_microusd: null,
      num_turns: null,
      duration_ms: null,
      max_output_tokens: null,
    };
    const summary = aggregateSessionSummary([process]);
    expect(summary.cacheHitRate).toBeNull();
    // Must not be NaN
    expect(Number.isNaN(summary.cacheHitRate ?? 0)).toBe(false);
  });

  it('computes cache hit rate correctly', () => {
    // total = 100, output = 10, cacheCreation = 20, cacheRead = 60, freshInput = 10
    // denominator = 10 + 20 + 60 = 90, rate = 60/90 = 67%
    const process: TokenUsageInfo = {
      total_tokens: 100n,
      model_context_window: 400000n,
      output_tokens: 10n,
      cache_creation_tokens: 20n,
      cache_read_tokens: 60n,
      cost_microusd: null,
      num_turns: null,
      duration_ms: null,
      max_output_tokens: null,
    };
    const summary = aggregateSessionSummary([process]);
    expect(summary.cacheHitRate).toBe(67);
    expect(summary.executorSupportsTokens).toBe(true);
  });

  it('sets executorSupportsTokens true when entries exist', () => {
    const process: TokenUsageInfo = {
      total_tokens: 500n,
      model_context_window: 200000n,
    };
    const summary = aggregateSessionSummary([process]);
    expect(summary.executorSupportsTokens).toBe(true);
  });

  // US-003: Inference-based capability tests
  it('empty entries + CLAUDE_CODE executor → executorSupportsTokens true', () => {
    const summary = aggregateSessionSummary([], 'CLAUDE_CODE');
    expect(summary.executorSupportsTokens).toBe(true);
    expect(summary.executorName).toBe('CLAUDE_CODE');
    expect(summary.contextTokens).toBe(0);
    expect(summary.contextWindow).toBe(0);
    expect(summary.outputTokens).toBeNull();
    expect(summary.numTurns).toBeNull();
  });

  it('empty entries + GEMINI executor → executorSupportsTokens false', () => {
    const summary = aggregateSessionSummary([], 'GEMINI');
    expect(summary.executorSupportsTokens).toBe(false);
    expect(summary.executorName).toBe('GEMINI');
  });

  it('empty entries + null executor → executorSupportsTokens false', () => {
    const summary = aggregateSessionSummary([], null);
    expect(summary.executorSupportsTokens).toBe(false);
    expect(summary.executorName).toBeNull();
  });

  it('non-empty entries + GEMINI executor → executorSupportsTokens true (observed emission wins)', () => {
    const validInfo: TokenUsageInfo = {
      total_tokens: 1000n,
      model_context_window: 200000n,
      output_tokens: 100n,
      cache_creation_tokens: null,
      cache_read_tokens: null,
      cost_microusd: null,
      num_turns: null,
      duration_ms: null,
      max_output_tokens: null,
    };
    const summary = aggregateSessionSummary([validInfo], 'GEMINI');
    expect(summary.executorSupportsTokens).toBe(true);
    expect(summary.executorName).toBe('GEMINI');
    expect(summary.contextTokens).toBe(1000);
  });

  // Legacy session test (H3 graceful degradation)
  it('legacy session with only total_tokens + model_context_window → graceful degradation', () => {
    const legacy: TokenUsageInfo = {
      total_tokens: 12_345n,
      model_context_window: 200_000n,
      output_tokens: null,
      cache_creation_tokens: null,
      cache_read_tokens: null,
      cost_microusd: null,
      num_turns: null,
      duration_ms: null,
      max_output_tokens: null,
    };
    const summary = aggregateSessionSummary([legacy], 'CLAUDE_CODE');
    expect(summary.contextTokens).toBe(12345);
    expect(summary.contextWindow).toBe(200000);
    expect(summary.costUSD).toBeNull();
    expect(summary.numTurns).toBeNull();
    expect(summary.durationMs).toBeNull();
    expect(summary.cacheHitRate).toBeNull();
    expect(summary.executorSupportsTokens).toBe(true);
    // Verify no NaN in any numeric field
    expect(Number.isNaN(summary.contextTokens)).toBe(false);
    expect(Number.isNaN(summary.contextWindow)).toBe(false);
  });
});
