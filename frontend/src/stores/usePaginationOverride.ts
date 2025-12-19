import { create } from 'zustand';

/**
 * Store for per-conversation pagination overrides.
 * Allows users to temporarily override the global pagination settings
 * for a specific conversation/attempt.
 */

export type PaginationPreset = 50 | 100 | 200 | 500 | 'global';

type State = {
  /**
   * Map of execution_id -> pagination override value
   * 'global' means use the global setting from config
   */
  overrides: Record<string, PaginationPreset>;

  /**
   * Set pagination override for a specific execution
   */
  setOverride: (executionId: string, value: PaginationPreset) => void;

  /**
   * Get the effective pagination limit for an execution
   * Returns the override if set, or 'global' if not
   */
  getOverride: (executionId: string) => PaginationPreset;

  /**
   * Clear override for a specific execution (revert to global)
   */
  clearOverride: (executionId: string) => void;

  /**
   * Clear all overrides
   */
  clearAll: () => void;
};

export const usePaginationOverrideStore = create<State>((set, get) => ({
  overrides: {},

  setOverride: (executionId, value) =>
    set((state) => ({
      overrides: { ...state.overrides, [executionId]: value },
    })),

  getOverride: (executionId) => {
    const { overrides } = get();
    return overrides[executionId] ?? 'global';
  },

  clearOverride: (executionId) =>
    set((state) => {
      const { [executionId]: _, ...rest } = state.overrides;
      return { overrides: rest };
    }),

  clearAll: () => set({ overrides: {} }),
}));

/**
 * Hook to get and set pagination override for a specific execution
 */
export function usePaginationOverride(
  executionId: string
): [PaginationPreset, (value: PaginationPreset) => void] {
  const override = usePaginationOverrideStore(
    (s) => s.overrides[executionId] ?? 'global'
  );
  const setOverride = usePaginationOverrideStore((s) => s.setOverride);

  return [override, (value) => setOverride(executionId, value)];
}

/**
 * Get the effective initial load count for an execution,
 * considering both the override and global config.
 */
export function getEffectiveInitialLoad(
  override: PaginationPreset,
  globalInitialLoad: number
): number {
  if (override === 'global') {
    return globalInitialLoad;
  }
  return override;
}
