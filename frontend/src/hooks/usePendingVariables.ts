import { useState, useCallback, useEffect, useMemo } from 'react';

/**
 * A pending variable that will be created after task creation
 */
export interface PendingVariable {
  id: string; // Local ID for tracking in UI
  name: string;
  value: string;
}

/**
 * Storage key prefix for pending variables in localStorage
 */
const STORAGE_KEY_PREFIX = 'vk_pending_variables_';

/**
 * Generate a unique session ID for the form
 */
const generateSessionId = (): string => {
  return `${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;
};

/**
 * Hook for buffering task variables before task creation.
 *
 * Variables cannot be added to a task until it exists (has an ID).
 * This hook provides a localStorage-backed buffer to hold variables
 * during task creation, which are then created after the task is saved.
 *
 * @param sessionId - Optional session ID to resume a previous session
 * @returns Object with variable management functions
 *
 * @example
 * ```tsx
 * function TaskForm() {
 *   const pendingVars = usePendingVariables();
 *
 *   const handleAddVariable = () => {
 *     pendingVars.addVariable({ name: 'API_KEY', value: 'sk-xxx' });
 *   };
 *
 *   const handleSubmit = async () => {
 *     const task = await createTask(taskData);
 *     if (pendingVars.hasItems()) {
 *       await bulkCreateVariables(task.id, pendingVars.getAll());
 *       pendingVars.clear();
 *     }
 *   };
 *
 *   return (
 *     <form onSubmit={handleSubmit}>
 *       {pendingVars.variables.map(v => (
 *         <div key={v.id}>{v.name}: {v.value}</div>
 *       ))}
 *       <button onClick={handleAddVariable}>Add Variable</button>
 *     </form>
 *   );
 * }
 * ```
 */
export function usePendingVariables(initialSessionId?: string) {
  // Generate or use provided session ID
  const [sessionId] = useState<string>(() => initialSessionId || generateSessionId());
  const storageKey = `${STORAGE_KEY_PREFIX}${sessionId}`;

  // Initialize state from localStorage
  const [variables, setVariables] = useState<PendingVariable[]>(() => {
    if (typeof window === 'undefined') return [];
    try {
      const stored = localStorage.getItem(storageKey);
      return stored ? JSON.parse(stored) : [];
    } catch {
      return [];
    }
  });

  // Persist to localStorage on changes
  useEffect(() => {
    if (typeof window === 'undefined') return;
    try {
      if (variables.length > 0) {
        localStorage.setItem(storageKey, JSON.stringify(variables));
      } else {
        localStorage.removeItem(storageKey);
      }
    } catch (e) {
      console.error('Failed to persist pending variables:', e);
    }
  }, [variables, storageKey]);

  // Add a new variable
  const addVariable = useCallback(
    (variable: Omit<PendingVariable, 'id'>) => {
      const newVariable: PendingVariable = {
        ...variable,
        id: `pending-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`,
      };
      setVariables((prev) => [...prev, newVariable]);
      return newVariable.id;
    },
    []
  );

  // Update an existing variable
  const updateVariable = useCallback(
    (id: string, updates: Partial<Omit<PendingVariable, 'id'>>) => {
      setVariables((prev) =>
        prev.map((v) => (v.id === id ? { ...v, ...updates } : v))
      );
    },
    []
  );

  // Remove a variable
  const removeVariable = useCallback((id: string) => {
    setVariables((prev) => prev.filter((v) => v.id !== id));
  }, []);

  // Clear all variables (call on form close/cancel)
  const clear = useCallback(() => {
    setVariables([]);
    if (typeof window !== 'undefined') {
      try {
        localStorage.removeItem(storageKey);
      } catch (e) {
        console.error('Failed to clear pending variables:', e);
      }
    }
  }, [storageKey]);

  // Check if there are any pending variables
  const hasItems = useCallback(() => variables.length > 0, [variables]);

  // Get all variables for submission
  const getAll = useCallback(
    () =>
      variables.map(({ name, value }) => ({
        name,
        value,
      })),
    [variables]
  );

  // Find a variable by name
  const findByName = useCallback(
    (name: string) => variables.find((v) => v.name === name),
    [variables]
  );

  // Check if a variable name already exists
  const nameExists = useCallback(
    (name: string, excludeId?: string) =>
      variables.some((v) => v.name === name && v.id !== excludeId),
    [variables]
  );

  // Memoize the return object to avoid unnecessary re-renders
  return useMemo(
    () => ({
      sessionId,
      variables,
      addVariable,
      updateVariable,
      removeVariable,
      clear,
      hasItems,
      getAll,
      findByName,
      nameExists,
    }),
    [
      sessionId,
      variables,
      addVariable,
      updateVariable,
      removeVariable,
      clear,
      hasItems,
      getAll,
      findByName,
      nameExists,
    ]
  );
}

export type UsePendingVariablesReturn = ReturnType<typeof usePendingVariables>;
