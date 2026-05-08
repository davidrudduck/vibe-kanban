/**
 * Pure aggregation logic for session token summary.
 * Extracted to a separate module so it can be unit-tested without HMR context dependencies.
 */
import type { TokenUsageInfo } from 'shared/types';

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
  executorSupportsTokens: boolean;
  executorName: string | null;
};

/** Compute aggregated session summary from per-process token usage entries. */
export function aggregateSessionSummary(
  entries: TokenUsageInfo[]
): SessionSummary {
  if (entries.length === 0) {
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
      executorSupportsTokens: false,
      executorName: null,
    };
  }

  // Latest process for snapshot fields
  const latest = entries[entries.length - 1];
  const contextTokens = Number(latest.total_tokens);
  const contextWindow = Number(latest.model_context_window);
  const maxOutputTokens =
    latest.max_output_tokens != null ? Number(latest.max_output_tokens) : null;

  // Cumulative sums across all processes
  let outputTokens: number | null = null;
  let cacheCreationTokens: number | null = null;
  let cacheReadTokens: number | null = null;
  let costMicroUSD: bigint | null = null;
  let numTurns: number | null = null;
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
    if (info.num_turns != null) {
      numTurns = (numTurns ?? 0) + info.num_turns;
    }
    if (info.duration_ms != null) {
      durationMs = (durationMs ?? 0) + Number(info.duration_ms);
    }
  }

  const costUSD =
    costMicroUSD != null ? Number(costMicroUSD) / 1_000_000 : null;

  // Cache hit rate: cacheRead / (freshInput + cacheCreation + cacheRead)
  // freshInput = total - output - cacheCreation - cacheRead
  // Guard: return null when denominator is 0
  let cacheHitRate: number | null = null;
  if (
    cacheReadTokens !== null &&
    cacheCreationTokens !== null &&
    outputTokens !== null
  ) {
    const freshInput =
      contextTokens - outputTokens - cacheCreationTokens - cacheReadTokens;
    const denominator =
      Math.max(0, freshInput) + cacheCreationTokens + cacheReadTokens;
    if (denominator > 0) {
      cacheHitRate = Math.round((cacheReadTokens / denominator) * 100);
    }
  }

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
    executorSupportsTokens: true,
    executorName: null,
  };
}
