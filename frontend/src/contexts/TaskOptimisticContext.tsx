import { createContext, useContext, useEffect, type ReactNode } from 'react';
import type { TaskStatus, TaskWithAttemptStatus } from 'shared/types';

interface TaskOptimisticContextType {
  /**
   * Optimistically add a task to local WebSocket state.
   * Call this after successful REST API creation for instant UI feedback.
   */
  addTaskOptimistically: (task: TaskWithAttemptStatus) => void;
  /**
   * Optimistically update a task's status in local WebSocket state.
   * Call this after successful REST API status update for instant UI feedback.
   */
  updateTaskStatusOptimistically: (taskId: string, status: TaskStatus) => void;
}

const TaskOptimisticContext = createContext<TaskOptimisticContextType | null>(
  null
);

interface TaskOptimisticProviderProps {
  children: ReactNode;
  addTaskOptimistically: (task: TaskWithAttemptStatus) => void;
  updateTaskStatusOptimistically: (taskId: string, status: TaskStatus) => void;
}

export function TaskOptimisticProvider({
  children,
  addTaskOptimistically,
  updateTaskStatusOptimistically,
}: TaskOptimisticProviderProps) {
  return (
    <TaskOptimisticContext.Provider
      value={{ addTaskOptimistically, updateTaskStatusOptimistically }}
    >
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

type AddTaskCallback = (task: TaskWithAttemptStatus) => void;
type UpdateStatusCallback = (taskId: string, status: TaskStatus) => void;

// Global registry of project -> callback mappings
const globalAddCallbackRegistry = new Map<string, AddTaskCallback>();
const globalStatusCallbackRegistry = new Map<string, UpdateStatusCallback>();

/**
 * Register an optimistic add callback for a project.
 * Call this from useProjectTasks when the hook mounts.
 */
export function registerOptimisticCallback(
  projectId: string,
  callback: AddTaskCallback
): void {
  globalAddCallbackRegistry.set(projectId, callback);
}

/**
 * Register an optimistic status update callback for a project.
 * Call this from useProjectTasks when the hook mounts.
 */
export function registerStatusCallback(
  projectId: string,
  callback: UpdateStatusCallback
): void {
  globalStatusCallbackRegistry.set(projectId, callback);
}

/**
 * Unregister an optimistic callback for a project.
 * Call this from useProjectTasks when the hook unmounts.
 */
export function unregisterOptimisticCallback(projectId: string): void {
  globalAddCallbackRegistry.delete(projectId);
}

/**
 * Unregister an optimistic status callback for a project.
 * Call this from useProjectTasks when the hook unmounts.
 */
export function unregisterStatusCallback(projectId: string): void {
  globalStatusCallbackRegistry.delete(projectId);
}

/**
 * Get the optimistic add callback for a project.
 * Used by useTaskMutations when context is not available (e.g., in modals).
 */
export function getOptimisticCallback(
  projectId: string
): AddTaskCallback | undefined {
  return globalAddCallbackRegistry.get(projectId);
}

/**
 * Get the optimistic status callback for a project.
 * Used by useFollowUpSend when context is not available.
 */
export function getStatusCallback(
  projectId: string
): UpdateStatusCallback | undefined {
  return globalStatusCallbackRegistry.get(projectId);
}

/**
 * Hook to register and auto-cleanup optimistic callback.
 * Use this in useProjectTasks.
 */
export function useRegisterOptimisticCallback(
  projectId: string | undefined,
  callback: AddTaskCallback
): void {
  useEffect(() => {
    if (!projectId) return;
    registerOptimisticCallback(projectId, callback);
    return () => {
      unregisterOptimisticCallback(projectId);
    };
  }, [projectId, callback]);
}

/**
 * Hook to register and auto-cleanup optimistic status callback.
 * Use this in useProjectTasks.
 */
export function useRegisterStatusCallback(
  projectId: string | undefined,
  callback: UpdateStatusCallback
): void {
  useEffect(() => {
    if (!projectId) return;
    registerStatusCallback(projectId, callback);
    return () => {
      unregisterStatusCallback(projectId);
    };
  }, [projectId, callback]);
}
