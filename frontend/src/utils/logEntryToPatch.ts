/**
 * Converts LogEntry[] from the unified logs API to PatchTypeWithKey[] for display.
 *
 * The unified logs API returns LogEntry objects where entries with output_type 'json_patch'
 * contain serialized RFC6902 JSON patch operations. These patches, when applied sequentially,
 * build up the PatchType[] array used by VirtualizedList.
 *
 * This utility applies those patches to reconstruct the display entries.
 */
import type { LogEntry, PatchType, NormalizedEntry } from 'shared/types';
import { applyPatch, type Operation } from 'rfc6902';

export type PatchTypeWithKey = PatchType & {
  patchKey: string;
  executionProcessId: string;
};

interface PatchContainer {
  entries: PatchType[];
}

/**
 * Applies a single LogEntry's JSON patch to the container.
 * Returns true if the patch was successfully applied.
 */
function applyLogEntryPatch(
  container: PatchContainer,
  entry: LogEntry
): boolean {
  if (entry.output_type !== 'json_patch') {
    return false;
  }

  try {
    const operations: Operation[] = JSON.parse(entry.content);
    if (!Array.isArray(operations)) {
      return false;
    }

    // Apply the patch operations to the container
    applyPatch(container as unknown as object, operations);
    return true;
  } catch (err) {
    console.warn('Failed to apply JSON patch:', err);
    return false;
  }
}

/**
 * Converts an array of LogEntry objects to PatchTypeWithKey[] for display.
 *
 * This function processes all LogEntry items with output_type 'json_patch',
 * applying their patches sequentially to build the final entries array.
 *
 * @param logEntries - The log entries from the unified logs API
 * @param executionProcessId - The execution process ID (used for keys)
 * @returns The transformed entries ready for VirtualizedList
 */
export function logEntriesToPatches(
  logEntries: LogEntry[],
  executionProcessId: string
): PatchTypeWithKey[] {
  const container: PatchContainer = { entries: [] };

  // Process entries in order, applying JSON patches
  for (const entry of logEntries) {
    if (entry.output_type === 'json_patch') {
      applyLogEntryPatch(container, entry);
    }
    // Note: We only process json_patch entries here.
    // stdout/stderr entries are handled differently in the live stream
  }

  // Add keys to each entry - use Object.assign to preserve the discriminated union
  return container.entries.map(
    (patch, index) =>
      Object.assign({}, patch, {
        patchKey: `${executionProcessId}:${index}`,
        executionProcessId,
      }) as PatchTypeWithKey
  );
}

/**
 * Creates a loading patch entry for display during loading states.
 */
export function createLoadingPatch(
  executionProcessId: string
): PatchTypeWithKey {
  const base: PatchType = {
    type: 'NORMALIZED_ENTRY',
    content: {
      entry_type: {
        type: 'loading',
      },
      content: '',
      timestamp: null,
    } as NormalizedEntry,
  };
  return Object.assign({}, base, {
    patchKey: 'loading',
    executionProcessId,
  }) as PatchTypeWithKey;
}

/**
 * Creates a "next action" patch entry for display when no process is running.
 */
export function createNextActionPatch(
  executionProcessId: string,
  options: {
    failed: boolean;
    executionProcesses: number;
    needsSetup: boolean;
    setupHelpText?: string;
  }
): PatchTypeWithKey {
  const base: PatchType = {
    type: 'NORMALIZED_ENTRY',
    content: {
      entry_type: {
        type: 'next_action',
        failed: options.failed,
        execution_processes: options.executionProcesses,
        needs_setup: options.needsSetup,
      },
      content: '',
      timestamp: null,
      metadata: null,
    },
  };
  return Object.assign({}, base, {
    patchKey: 'next_action',
    executionProcessId,
  }) as PatchTypeWithKey;
}

/**
 * Creates a user message patch entry.
 */
export function createUserMessagePatch(
  executionProcessId: string,
  prompt: string
): PatchTypeWithKey {
  const base: PatchType = {
    type: 'NORMALIZED_ENTRY',
    content: {
      entry_type: {
        type: 'user_message',
      },
      content: prompt,
      timestamp: null,
    } as NormalizedEntry,
  };
  return Object.assign({}, base, {
    patchKey: `${executionProcessId}:user`,
    executionProcessId,
  }) as PatchTypeWithKey;
}

/**
 * Converts a live LogEntry to a PatchType for immediate display.
 * Used for live streaming where entries come one at a time.
 */
export function logEntryToLivePatch(
  entry: LogEntry,
  executionProcessId: string,
  index: number
): PatchTypeWithKey | null {
  let base: PatchType;

  switch (entry.output_type) {
    case 'stdout':
      base = {
        type: 'STDOUT',
        content: entry.content,
      };
      return Object.assign({}, base, {
        patchKey: `${executionProcessId}:live:${index}`,
        executionProcessId,
      }) as PatchTypeWithKey;
    case 'stderr':
      base = {
        type: 'STDERR',
        content: entry.content,
      };
      return Object.assign({}, base, {
        patchKey: `${executionProcessId}:live:${index}`,
        executionProcessId,
      }) as PatchTypeWithKey;
    case 'json_patch':
      // For live streaming, we don't process json_patch entries here
      // They are handled by the main logEntriesToPatches function
      return null;
    default:
      return null;
  }
}
