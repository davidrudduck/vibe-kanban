import { BaseCodingAgent } from 'shared/types';
import type {
  ExecutorConfig,
  ExecutorConfigs,
  ExecutorProfile,
  ExecutorAction,
  ExecutorProfileId,
  ExecutionProcess,
} from 'shared/types';
import { toPrettyCase } from './string';

const RESERVED_KEYS = new Set(['recently_used_models']);

export const CLAUDE_CODE_SDK_WARNING =
  'Uses Claude Code print/SDK mode. Starting June 15, 2026, Anthropic bills this separately from Claude Pro/Max subscription usage.';

export const CLAUDE_CODE_TERMINAL_NOTE =
  'Runs native Claude Code in a tmux terminal. Existing Claude settings, plugins, and hooks remain active unless isolated settings are selected.';

export const EXECUTOR_DISPLAY_NAMES: Partial<Record<BaseCodingAgent, string>> =
  {
    [BaseCodingAgent.CLAUDE_CODE]: 'Claude Code SDK',
    [BaseCodingAgent.CLAUDE_TERMINAL]: 'Claude Code Terminal',
    [BaseCodingAgent.CODEX]: 'Codex',
    [BaseCodingAgent.OPENCODE]: 'OpenCode',
    [BaseCodingAgent.CURSOR_AGENT]: 'Cursor Agent',
    [BaseCodingAgent.QWEN_CODE]: 'Qwen Code',
    [BaseCodingAgent.GEMINI]: 'Gemini',
    [BaseCodingAgent.AMP]: 'Amp',
    [BaseCodingAgent.COPILOT]: 'Copilot',
    [BaseCodingAgent.DROID]: 'Droid',
  };

export function getExecutorDisplayName(
  executor: BaseCodingAgent | string | null | undefined
): string {
  if (!executor) return 'Agent';
  return (
    EXECUTOR_DISPLAY_NAMES[executor as BaseCodingAgent] ??
    toPrettyCase(executor)
  );
}

export function getExecutorModeDescription(
  executor: BaseCodingAgent | string | null | undefined
): string | null {
  switch (executor) {
    case BaseCodingAgent.CLAUDE_CODE:
      return CLAUDE_CODE_SDK_WARNING;
    case BaseCodingAgent.CLAUDE_TERMINAL:
      return CLAUDE_CODE_TERMINAL_NOTE;
    default:
      return null;
  }
}

export function getExecutorVariantKeys(
  executorProfile: ExecutorProfile | Record<string, unknown> | null | undefined
): string[] {
  return Object.keys(executorProfile || {}).filter(
    (key) => !RESERVED_KEYS.has(key)
  );
}

function sortVariantKeys(variants: string[]): string[] {
  return variants.sort((a, b) => {
    if (a === 'DEFAULT') return -1;
    if (b === 'DEFAULT') return 1;
    return a.localeCompare(b);
  });
}

export function getSortedExecutorVariantKeys(
  executorProfile: ExecutorProfile | Record<string, unknown> | null | undefined
): string[] {
  return sortVariantKeys(getExecutorVariantKeys(executorProfile));
}

/**
 * Compare two ExecutorProfileIds for equality.
 * Treats null/undefined variant as equivalent to "DEFAULT".
 */
export function areProfilesEqual(
  a: ExecutorProfileId | null | undefined,
  b: ExecutorProfileId | null | undefined
): boolean {
  if (!a || !b) return a === b;
  if (a.executor !== b.executor) return false;
  // Normalize variants: null/undefined -> 'DEFAULT'
  const variantA = a.variant ?? 'DEFAULT';
  const variantB = b.variant ?? 'DEFAULT';
  return variantA === variantB;
}

/**
 * Get variant options for a given executor from profiles.
 * Returns variants sorted: DEFAULT first, then alphabetically.
 */
export function getVariantOptions(
  executor: BaseCodingAgent | null | undefined,
  profiles: ExecutorConfigs['executors'] | null | undefined
): string[] {
  if (!executor || !profiles) return [];
  const executorProfile = profiles[executor];
  if (!executorProfile) return [];

  const variants = getExecutorVariantKeys(executorProfile);
  return sortVariantKeys(variants);
}

/**
 * Extract full ExecutorConfig from an ExecutorAction chain.
 * Traverses the action chain to find the first coding agent request.
 */
export function executorConfigFromAction(
  action: ExecutorAction | null
): ExecutorConfig | null {
  let curr: ExecutorAction | null = action;
  while (curr) {
    const typ = curr.typ;
    switch (typ.type) {
      case 'CodingAgentInitialRequest':
      case 'CodingAgentFollowUpRequest':
      case 'ReviewRequest':
        return typ.executor_config;
      case 'ScriptRequest':
      default:
        curr = curr.next_action;
        continue;
    }
  }
  return null;
}

/**
 * Get the full ExecutorConfig from the most recent execution process.
 * Searches from most recent to oldest.
 */
export function getLatestConfigFromProcesses(
  processes: ExecutionProcess[] | undefined
): ExecutorConfig | null {
  if (!processes?.length) return null;
  return (
    processes
      .slice()
      .reverse()
      .map((p) => executorConfigFromAction(p.executor_action ?? null))
      .find((c) => c !== null) ?? null
  );
}
