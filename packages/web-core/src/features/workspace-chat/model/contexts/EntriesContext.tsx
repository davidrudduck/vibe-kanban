import { useContext, useState, useMemo, useCallback, ReactNode } from 'react';
import { createHmrContext } from '@/shared/lib/hmrContext';
import type { PatchTypeWithKey } from '@/shared/hooks/useConversationHistory/types';
import type { TokenUsageInfo } from 'shared/types';
import {
  aggregateSessionSummary,
  type SessionSummary,
} from '../sessionSummary';
import { useExecutionProcessesContext } from '@/shared/hooks/useExecutionProcessesContext';
import { getLatestConfigFromProcesses } from '@/shared/lib/executor';

// ---------------------------------------------------------------------------
// Entries context — changes on every streaming update
// ---------------------------------------------------------------------------

interface EntriesContextType {
  entries: PatchTypeWithKey[];
  setEntries: (entries: PatchTypeWithKey[]) => void;
  reset: () => void;
}

interface EntriesActionsContextType {
  setEntries: (entries: PatchTypeWithKey[]) => void;
  reset: () => void;
}

const EntriesContext = createHmrContext<EntriesContextType | null>(
  'EntriesContext',
  null
);

const EntriesActionsContext =
  createHmrContext<EntriesActionsContextType | null>(
    'EntriesActionsContext',
    null
  );

// ---------------------------------------------------------------------------
// Token-usage context — changes only when token stats update (much rarer)
// ---------------------------------------------------------------------------

interface TokenUsageContextType {
  tokenUsageInfo: TokenUsageInfo | null;
  setTokenUsageInfo: (info: TokenUsageInfo | null) => void;
}

const TokenUsageContext = createHmrContext<TokenUsageContextType | null>(
  'TokenUsageContext',
  null
);

// ---------------------------------------------------------------------------
// Per-process token usage map — for aggregated SESSION panel
// ---------------------------------------------------------------------------

// SessionSummary type and aggregateSessionSummary function are imported from
// ../sessionSummary (kept separate so they can be unit-tested without HMR context)
export type { SessionSummary } from '../sessionSummary';
export { aggregateSessionSummary } from '../sessionSummary';

// Empty map constant for memoization stability when provider is unavailable
const EMPTY_TOKEN_MAP: ReadonlyMap<string, TokenUsageInfo> = new Map();

interface TokenUsageMapContextType {
  tokenUsageByProcess: Map<string, TokenUsageInfo>;
  setTokenUsageByProcess: (map: Map<string, TokenUsageInfo>) => void;
}

const TokenUsageMapContext = createHmrContext<TokenUsageMapContextType | null>(
  'TokenUsageMapContext',
  null
);

// ---------------------------------------------------------------------------
// Provider — nested contexts, single component
// ---------------------------------------------------------------------------

interface EntriesProviderProps {
  children: ReactNode;
}

export const EntriesProvider = ({ children }: EntriesProviderProps) => {
  const [entries, setEntriesState] = useState<PatchTypeWithKey[]>([]);
  const [tokenUsageInfo, setTokenUsageInfoState] =
    useState<TokenUsageInfo | null>(null);
  const [tokenUsageByProcess, setTokenUsageByProcessState] = useState<
    Map<string, TokenUsageInfo>
  >(new Map());

  const setEntries = useCallback((newEntries: PatchTypeWithKey[]) => {
    setEntriesState(newEntries);
  }, []);

  const setTokenUsageInfo = useCallback((info: TokenUsageInfo | null) => {
    setTokenUsageInfoState(info);
  }, []);

  const setTokenUsageByProcess = useCallback(
    (map: Map<string, TokenUsageInfo>) => {
      setTokenUsageByProcessState(map);
    },
    []
  );

  const reset = useCallback(() => {
    setEntriesState([]);
    setTokenUsageInfoState(null);
    setTokenUsageByProcessState(new Map());
  }, []);

  const entriesValue = useMemo(
    () => ({ entries, setEntries, reset }),
    [entries, setEntries, reset]
  );

  const entriesActionsValue = useMemo(
    () => ({ setEntries, reset }),
    [setEntries, reset]
  );

  const tokenUsageValue = useMemo(
    () => ({ tokenUsageInfo, setTokenUsageInfo }),
    [tokenUsageInfo, setTokenUsageInfo]
  );

  const tokenUsageMapValue = useMemo(
    () => ({ tokenUsageByProcess, setTokenUsageByProcess }),
    [tokenUsageByProcess, setTokenUsageByProcess]
  );

  return (
    <EntriesActionsContext.Provider value={entriesActionsValue}>
      <EntriesContext.Provider value={entriesValue}>
        <TokenUsageContext.Provider value={tokenUsageValue}>
          <TokenUsageMapContext.Provider value={tokenUsageMapValue}>
            {children}
          </TokenUsageMapContext.Provider>
        </TokenUsageContext.Provider>
      </EntriesContext.Provider>
    </EntriesActionsContext.Provider>
  );
};

// ---------------------------------------------------------------------------
// Hooks
// ---------------------------------------------------------------------------

export const useEntries = (): EntriesContextType => {
  const context = useContext(EntriesContext);
  if (!context) {
    throw new Error('useEntries must be used within an EntriesProvider');
  }
  return context;
};

export const useEntriesActions = (): EntriesActionsContextType => {
  const context = useContext(EntriesActionsContext);
  if (!context) {
    throw new Error('useEntriesActions must be used within an EntriesProvider');
  }
  return context;
};

/**
 * Read token-usage info without subscribing to entries changes.
 * This context only updates when the token stats themselves change,
 * not on every streaming entry update.
 */
export const useTokenUsage = (): TokenUsageInfo | null => {
  const context = useContext(TokenUsageContext);
  if (!context) {
    throw new Error('useTokenUsage must be used within an EntriesProvider');
  }
  return context.tokenUsageInfo;
};

/**
 * Get the setTokenUsageInfo setter without subscribing to entries.
 * Used by useConversationHistory to push token stats into context.
 */
export const useSetTokenUsageInfo = (): ((
  info: TokenUsageInfo | null
) => void) => {
  const context = useContext(TokenUsageContext);
  if (!context) {
    throw new Error(
      'useSetTokenUsageInfo must be used within an EntriesProvider'
    );
  }
  return context.setTokenUsageInfo;
};

/**
 * Returns the per-process token usage map.
 * Safe to call outside EntriesProvider (returns stable empty Map for memoization).
 */
export const useTokenUsageByProcess = (): ReadonlyMap<
  string,
  TokenUsageInfo
> => {
  const context = useContext(TokenUsageMapContext);
  return context?.tokenUsageByProcess ?? EMPTY_TOKEN_MAP;
};

const _noop = (_map: Map<string, TokenUsageInfo>) => {};

/**
 * Returns the setter for the per-process token usage map.
 * Safe to call outside EntriesProvider (returns a no-op).
 */
export const useSetTokenUsageByProcess = (): ((
  map: Map<string, TokenUsageInfo>
) => void) => {
  const context = useContext(TokenUsageMapContext);
  return context?.setTokenUsageByProcess ?? _noop;
};

export const useSessionSummary = (): SessionSummary => {
  const byProcess = useTokenUsageByProcess();
  const { executionProcessesAll: processes } = useExecutionProcessesContext();
  const executor = useMemo(
    () => getLatestConfigFromProcesses(processes)?.executor ?? null,
    [processes]
  );
  return useMemo(() => {
    const entries = Array.from(byProcess.values());
    return aggregateSessionSummary(entries, executor);
  }, [byProcess, executor]);
};
