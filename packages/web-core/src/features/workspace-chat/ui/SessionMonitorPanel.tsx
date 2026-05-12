import { useSessionSummary } from '../model/contexts/EntriesContext';
import {
  formatTokenCount as formatTokens,
  formatMsDuration as formatDuration,
} from '../model/sessionSummary';

export function SessionMonitorPanel() {
  const summary = useSessionSummary();

  // Empty state detection: use hasEntries flag instead of inferring from values
  // This prevents flicker when entries have been emitted but values are still zero
  const hasNoData = !summary.hasEntries;

  // Branch 1: Empty state - waiting for first turn (executor supports but no data yet)
  if (hasNoData && summary.executorSupportsTokens) {
    return (
      <div
        className="p-3 text-xs text-muted-foreground space-y-1"
        data-testid="session-monitor-waiting"
      >
        <p>Waiting for first turn…</p>
        <p className="opacity-60">
          Token usage will appear after the first response.
        </p>
      </div>
    );
  }

  // Branch 2: Empty state - telemetry not supported (executor doesn't emit tokens)
  if (hasNoData && !summary.executorSupportsTokens) {
    return (
      <div
        className="p-3 text-xs text-muted-foreground space-y-1"
        data-testid="session-monitor-not-supported"
      >
        <p>
          Telemetry not available for {summary.executorName ?? 'this executor'}.
        </p>
        <p className="opacity-60">Context window monitoring only.</p>
      </div>
    );
  }

  // Branch 3: Populated state - we have token usage data
  const contextWindowKnown = summary.contextWindow > 0;
  const contextPct = contextWindowKnown
    ? Math.min(
        100,
        Math.round((summary.contextTokens / summary.contextWindow) * 100)
      )
    : null;

  return (
    <div
      className="flex flex-col gap-3 p-3 text-xs"
      data-testid="session-monitor-populated"
    >
      {/* Context */}
      <div>
        <p className="font-medium text-foreground mb-1.5">Context</p>
        {contextWindowKnown ? (
          <div className="flex items-center gap-2">
            <div className="flex-1 bg-muted rounded-full h-1.5 overflow-hidden">
              <div
                className="bg-primary h-full rounded-full transition-all duration-300"
                style={{ width: `${contextPct}%` }}
              />
            </div>
            <span className="text-muted-foreground whitespace-nowrap tabular-nums">
              {contextPct}% · {formatTokens(summary.contextTokens)}/
              {formatTokens(summary.contextWindow)}
            </span>
          </div>
        ) : (
          <div className="flex items-center gap-2">
            <div className="flex-1 bg-muted rounded-full h-1.5" />
            <span className="text-muted-foreground whitespace-nowrap tabular-nums">
              {formatTokens(summary.contextTokens)} used · window unknown
            </span>
          </div>
        )}
        {summary.maxOutputTokens !== null && (
          <p className="text-muted-foreground mt-1">
            Max output: {formatTokens(summary.maxOutputTokens)}
          </p>
        )}
      </div>

      {/* Cost */}
      {summary.costUSD !== null && (
        <div>
          <p className="font-medium text-foreground mb-0.5">Cost</p>
          <p className="text-muted-foreground">
            ${summary.costUSD.toFixed(2)} this session
          </p>
        </div>
      )}

      {/* Tokens */}
      <div>
        <p className="font-medium text-foreground mb-1">Tokens (cumulative)</p>
        <div className="flex flex-col gap-0.5 text-muted-foreground">
          {summary.cacheCreationTokens !== null && (
            <div className="flex justify-between">
              <span>Cache created:</span>
              <span className="tabular-nums">
                {formatTokens(summary.cacheCreationTokens)}
              </span>
            </div>
          )}
          {summary.cacheReadTokens !== null && (
            <div className="flex justify-between">
              <span>
                Cache read
                {summary.cacheHitRate !== null && (
                  <span className="ml-1 text-yellow-600 dark:text-yellow-400">
                    ⚡ {summary.cacheHitRate}%
                  </span>
                )}
                :
              </span>
              <span className="tabular-nums">
                {formatTokens(summary.cacheReadTokens)}
              </span>
            </div>
          )}
          {summary.outputTokens !== null && (
            <div className="flex justify-between">
              <span>Output:</span>
              <span className="tabular-nums">
                {formatTokens(summary.outputTokens)}
              </span>
            </div>
          )}
        </div>
      </div>

      {/* Session stats */}
      {(summary.numTurns !== null || summary.durationMs !== null) && (
        <div>
          <p className="font-medium text-foreground mb-0.5">Session</p>
          <p className="text-muted-foreground tabular-nums">
            {summary.numTurns !== null && `${summary.numTurns} turns`}
            {summary.numTurns !== null && summary.durationMs !== null && ' · '}
            {summary.durationMs !== null && formatDuration(summary.durationMs)}
          </p>
        </div>
      )}
    </div>
  );
}
