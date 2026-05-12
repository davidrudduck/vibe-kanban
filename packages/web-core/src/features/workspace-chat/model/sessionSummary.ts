/**
 * Pure aggregation logic for session token summary.
 * Extracted to a separate module so it can be unit-tested without HMR context dependencies.
 */
import type { TokenUsageInfo } from 'shared/types';
import { BaseCodingAgent } from 'shared/types';

/**
 * Executors known to emit TokenUsageInfo today. Source of truth:
 *   - CLAUDE_CODE: crates/executors/src/executors/claude.rs
 *   - CODEX:       crates/executors/src/executors/codex/normalize_logs.rs
 *   - OPENCODE:    crates/executors/src/executors/opencode/normalize_logs.rs
 * Other executors (AMP, GEMINI, CURSOR_AGENT, QWEN_CODE, COPILOT, DROID) do not
 * emit telemetry as of this revision. If they begin to, the panel will infer
 * support from observed entries automatically (entries.length > 0).
 *
 * See ADR: docs/adr/0001-rust-token-capability-flag.md for the planned migration
 * to a Rust-sourced BaseAgentCapability::TOKEN_USAGE flag.
 */
export const KNOWN_TOKEN_EMITTERS: ReadonlySet<BaseCodingAgent> = new Set([
  BaseCodingAgent.CLAUDE_CODE,
  BaseCodingAgent.CODEX,
  BaseCodingAgent.OPENCODE,
]);

// ---------------------------------------------------------------------------
// Display helpers — shared by SessionMonitorPanel and DisplayConversationEntry
// ---------------------------------------------------------------------------

/** Format a token count as a compact string (e.g. 2.06M, 92.8k, 72). Null → '—'. */
export function formatTokenCount(n: number | null): string {
  if (n === null) return '—';
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
}

/** Format a millisecond duration as m:ss or Ns. Null → '—'. */
export function formatMsDuration(ms: number | null): string {
  if (ms === null) return '—';
  const s = Math.round(ms / 1000);
  const m = Math.floor(s / 60);
  return m > 0 ? `${m}:${String(s % 60).padStart(2, '0')}` : `${s}s`;
}

/** Format executor name for display (SHOUTY_SNAKE_CASE → Title Case). Null → 'this executor'. */
export function formatExecutorName(executor: string | null): string {
  if (executor === null) return 'this executor';
  return executor
    .split('_')
    .map((word) => word.charAt(0) + word.slice(1).toLowerCase())
    .join(' ');
}

export type SessionSummary = {
  // Snapshot from latest process
  contextTokens: number;
  contextWindow: number;
  maxOutputTokens: number | null;
  // Cumulative across all processes
  outputTokens: number | null;
  cacheCreationTokens: number | null;
  cacheReadTokens: number | null;
  costUSD: number | null;
  numTurns: number | null;
  durationMs: number | null;
  // Derived
  cacheHitRate: number | null; // null if denominator is 0
  // Meta
  hasEntries: boolean; // true if any token entries have been emitted
  executorSupportsTokens: boolean;
  executorName: string | null;
};

/** Compute aggregated session summary from per-process token usage entries. */
export function aggregateSessionSummary(
  entries: TokenUsageInfo[],
  executor: BaseCodingAgent | null = null
): SessionSummary {
  if (entries.length === 0) {
    const executorSupportsTokens =
      executor !== null && KNOWN_TOKEN_EMITTERS.has(executor);
    return {
      contextTokens: 0,
      contextWindow: 0,
      maxOutputTokens: null,
      outputTokens: null,
      cacheCreationTokens: null,
      cacheReadTokens: null,
      costUSD: null,
      numTurns: null,
      durationMs: null,
      cacheHitRate: null,
      hasEntries: false,
      executorSupportsTokens,
      executorName: executor,
    };
  }

  // Latest process for snapshot fields
  const latest = entries[entries.length - 1];
  const contextTokens = Number(latest.total_tokens);
  const contextWindow = Number(latest.model_context_window);
  const maxOutputTokens =
    latest.max_output_tokens != null ? Number(latest.max_output_tokens) : null;

  // Cumulative sums across all processes (additive quantities only)
  let outputTokens: number | null = null;
  let cacheCreationTokens: number | null = null;
  let cacheReadTokens: number | null = null;
  let costMicroUSD: bigint | null = null;
  let durationMs: number | null = null;

  for (const info of entries) {
    if (info.output_tokens != null) {
      outputTokens = (outputTokens ?? 0) + Number(info.output_tokens);
    }
    if (info.cache_creation_tokens != null) {
      cacheCreationTokens =
        (cacheCreationTokens ?? 0) + Number(info.cache_creation_tokens);
    }
    if (info.cache_read_tokens != null) {
      cacheReadTokens = (cacheReadTokens ?? 0) + Number(info.cache_read_tokens);
    }
    if (info.cost_microusd != null) {
      costMicroUSD = (costMicroUSD ?? 0n) + info.cost_microusd;
    }
    if (info.duration_ms != null) {
      durationMs = (durationMs ?? 0) + Number(info.duration_ms);
    }
  }

  const costUSD =
    costMicroUSD != null ? Number(costMicroUSD) / 1_000_000 : null;

  // num_turns: use LATEST process only.
  // Claude's num_turns in the Result event already counts all turns in the
  // conversation (including resumed turns), so summing across processes
  // would double-count. Latest process wins.
  const numTurns = latest.num_turns ?? null;

  // Cache hit rate: computed from LATEST process snapshot only.
  // Mixing latest-process contextTokens with cumulative cache sums produces
  // a dimensionally inconsistent denominator in multi-process sessions.
  // Using only the latest entry's own fields keeps the calculation coherent.
  let cacheHitRate: number | null = null;
  const latestOutput =
    latest.output_tokens != null ? Number(latest.output_tokens) : null;
  const latestCacheCreation =
    latest.cache_creation_tokens != null
      ? Number(latest.cache_creation_tokens)
      : null;
  const latestCacheRead =
    latest.cache_read_tokens != null ? Number(latest.cache_read_tokens) : null;
  if (
    latestCacheRead !== null &&
    latestCacheCreation !== null &&
    latestOutput !== null
  ) {
    const freshInput =
      contextTokens - latestOutput - latestCacheCreation - latestCacheRead;
    const denominator =
      Math.max(0, freshInput) + latestCacheCreation + latestCacheRead;
    if (denominator > 0) {
      cacheHitRate = Math.round((latestCacheRead / denominator) * 100);
    }
  }

  // Populated branch: observed emission proves support
  const executorSupportsTokens = true;

  return {
    contextTokens,
    contextWindow,
    maxOutputTokens,
    outputTokens,
    cacheCreationTokens,
    cacheReadTokens,
    costUSD,
    numTurns,
    durationMs,
    cacheHitRate,
    hasEntries: true,
    executorSupportsTokens,
    executorName: executor,
  };
}
