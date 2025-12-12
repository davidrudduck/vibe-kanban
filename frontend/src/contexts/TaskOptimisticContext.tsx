import { createContext, useContext, useEffect, type ReactNode } from 'react';
import type { TaskWithAttemptStatus } from 'shared/types';

interface TaskOptimisticContextType {
  /**
   * Optimistically add a task to local WebSocket state.
   * Call this after successful REST API creation for instant UI feedback.
   */
  addTaskOptimistically: (task: TaskWithAttemptStatus) => void;
}

const TaskOptimisticContext = createContext<TaskOptimisticContextType | null>(
  null
);

interface TaskOptimisticProviderProps {
  children: ReactNode;
  addTaskOptimistically: (task: TaskWithAttemptStatus) => void;
}

export function TaskOptimisticProvider({
  children,
  addTaskOptimistically,
}: TaskOptimisticProviderProps) {
  return (
    <TaskOptimisticContext.Provider value={{ addTaskOptimistically }}>
      {children}
    </TaskOptimisticContext.Provider>
  );
}

/**
 * Hook to access task optimistic update function.
 * Returns undefined if not within a TaskOptimisticProvider.
 */
export function useTaskOptimistic(): TaskOptimisticContextType | null {
  return useContext(TaskOptimisticContext);
}

// ============================================================================
// Global Registry Pattern
// ============================================================================
// This allows components rendered outside the TaskOptimisticProvider (like modals)
// to still access the optimistic update callback for the current project.

type OptimisticCallback = (task: TaskWithAttemptStatus) => void;

// Global registry of project -> callback mappings
const globalCallbackRegistry = new Map<string, OptimisticCallback>();

/**
 * Register an optimistic callback for a project.
 * Call this from useProjectTasks when the hook mounts.
 */
export function registerOptimisticCallback(
  projectId: string,
  callback: OptimisticCallback
): void {
  globalCallbackRegistry.set(projectId, callback);
}

/**
 * Unregister an optimistic callback for a project.
 * Call this from useProjectTasks when the hook unmounts.
 */
export function unregisterOptimisticCallback(projectId: string): void {
  globalCallbackRegistry.delete(projectId);
}

/**
 * Get the optimistic callback for a project.
 * Used by useTaskMutations when context is not available (e.g., in modals).
 */
export function getOptimisticCallback(
  projectId: string
): OptimisticCallback | undefined {
  return globalCallbackRegistry.get(projectId);
}

/**
 * Hook to register and auto-cleanup optimistic callback.
 * Use this in useProjectTasks.
 */
export function useRegisterOptimisticCallback(
  projectId: string | undefined,
  callback: OptimisticCallback
): void {
  useEffect(() => {
    if (!projectId) return;
    registerOptimisticCallback(projectId, callback);
    return () => {
      unregisterOptimisticCallback(projectId);
    };
  }, [projectId, callback]);
}
