/**
 * useEffectivePagination - Get effective pagination settings for an execution
 *
 * Combines global config pagination settings with per-conversation overrides
 * to determine the effective pagination limit for a given execution.
 */
import { useUserSystem } from '@/components/ConfigProvider';
import {
  usePaginationOverride,
  getEffectiveInitialLoad,
} from '@/stores/usePaginationOverride';

const DEFAULT_INITIAL_LOAD = 100;

/**
 * Hook to get effective pagination settings for an execution
 *
 * @param executionId - The execution ID to get pagination for
 * @returns Object with effective initial load and helpers
 */
export function useEffectivePagination(executionId: string) {
  const { config } = useUserSystem();
  const [override, setOverride] = usePaginationOverride(executionId);

  // Get global initial load from config, fallback to default
  const globalInitialLoad = config?.pagination?.initial_load
    ? Number(config.pagination.initial_load)
    : DEFAULT_INITIAL_LOAD;

  // Calculate effective limit (override takes precedence over global)
  const effectiveLimit = getEffectiveInitialLoad(override, globalInitialLoad);

  return {
    /** The effective initial load limit for this execution */
    effectiveLimit,
    /** The global default from config */
    globalLimit: globalInitialLoad,
    /** The current override value ('global' means use global setting) */
    override,
    /** Set a per-conversation override */
    setOverride,
    /** Whether an override is active (not 'global') */
    hasOverride: override !== 'global',
  };
}

/**
 * Get the global pagination limit without per-conversation context
 * Useful for components that don't have an execution ID yet
 */
export function useGlobalPaginationLimit(): number {
  const { config } = useUserSystem();
  return config?.pagination?.initial_load
    ? Number(config.pagination.initial_load)
    : DEFAULT_INITIAL_LOAD;
}
