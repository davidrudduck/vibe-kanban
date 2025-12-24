import { useState, useCallback, useMemo } from 'react';

export interface TerminalTab {
  id: string;
  label: string;
  workingDir: string;
}

interface UseTerminalTabsOptions {
  /** Maximum number of terminals allowed */
  maxTabs?: number;
}

interface UseTerminalTabsReturn {
  tabs: TerminalTab[];
  activeTabId: string | null;
  addTab: (workingDir: string, label?: string) => string | null;
  removeTab: (tabId: string) => void;
  setActiveTab: (tabId: string) => void;
  navigateToPreviousTab: () => void;
  navigateToNextTab: () => void;
  getTab: (tabId: string) => TerminalTab | undefined;
  hasTab: (workingDir: string) => boolean;
  findTabByWorkingDir: (workingDir: string) => TerminalTab | undefined;
}

/**
 * Hook for managing multiple terminal tabs.
 * Supports adding, removing, and switching between terminals.
 */
export function useTerminalTabs(
  options: UseTerminalTabsOptions = {}
): UseTerminalTabsReturn {
  const { maxTabs = 5 } = options;

  const [tabs, setTabs] = useState<TerminalTab[]>([]);
  const [activeTabId, setActiveTabId] = useState<string | null>(null);

  const generateTabId = useCallback(() => {
    return `term-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
  }, []);

  const generateLabel = useCallback(
    (workingDir: string, existingTabs: TerminalTab[]) => {
      // Extract the last part of the path for a shorter label
      const parts = workingDir.split('/').filter(Boolean);
      const baseName = parts[parts.length - 1] || 'Terminal';

      // Check if a tab with this label already exists
      const existingLabels = existingTabs.map((t) => t.label);
      if (!existingLabels.includes(baseName)) {
        return baseName;
      }

      // Add a number suffix if needed
      let counter = 2;
      while (existingLabels.includes(`${baseName} (${counter})`)) {
        counter++;
      }
      return `${baseName} (${counter})`;
    },
    []
  );

  const addTab = useCallback(
    (workingDir: string, label?: string): string | null => {
      // Check if we've reached the max tabs
      if (tabs.length >= maxTabs) {
        console.warn(`Maximum number of terminals (${maxTabs}) reached`);
        return null;
      }

      const id = generateTabId();
      const tabLabel = label || generateLabel(workingDir, tabs);

      const newTab: TerminalTab = {
        id,
        label: tabLabel,
        workingDir,
      };

      setTabs((prev) => [...prev, newTab]);
      setActiveTabId(id);

      return id;
    },
    [tabs, maxTabs, generateTabId, generateLabel]
  );

  const removeTab = useCallback(
    (tabId: string) => {
      setTabs((prev) => {
        const index = prev.findIndex((t) => t.id === tabId);
        if (index === -1) return prev;

        const newTabs = prev.filter((t) => t.id !== tabId);

        // If we're removing the active tab, activate another one
        if (activeTabId === tabId && newTabs.length > 0) {
          // Prefer the tab to the left, or the first remaining tab
          const newActiveIndex = Math.max(0, index - 1);
          setActiveTabId(newTabs[newActiveIndex]?.id ?? null);
        } else if (newTabs.length === 0) {
          setActiveTabId(null);
        }

        return newTabs;
      });
    },
    [activeTabId]
  );

  const setActiveTab = useCallback((tabId: string) => {
    setActiveTabId(tabId);
  }, []);

  const navigateToPreviousTab = useCallback(() => {
    if (tabs.length <= 1) return;
    const currentIndex = tabs.findIndex((t) => t.id === activeTabId);
    if (currentIndex <= 0) return; // Already at first tab
    setActiveTabId(tabs[currentIndex - 1].id);
  }, [tabs, activeTabId]);

  const navigateToNextTab = useCallback(() => {
    if (tabs.length <= 1) return;
    const currentIndex = tabs.findIndex((t) => t.id === activeTabId);
    if (currentIndex === -1 || currentIndex >= tabs.length - 1) return; // Already at last tab
    setActiveTabId(tabs[currentIndex + 1].id);
  }, [tabs, activeTabId]);

  const getTab = useCallback(
    (tabId: string): TerminalTab | undefined => {
      return tabs.find((t) => t.id === tabId);
    },
    [tabs]
  );

  const hasTab = useCallback(
    (workingDir: string): boolean => {
      return tabs.some((t) => t.workingDir === workingDir);
    },
    [tabs]
  );

  const findTabByWorkingDir = useCallback(
    (workingDir: string): TerminalTab | undefined => {
      return tabs.find((t) => t.workingDir === workingDir);
    },
    [tabs]
  );

  return useMemo(
    () => ({
      tabs,
      activeTabId,
      addTab,
      removeTab,
      setActiveTab,
      navigateToPreviousTab,
      navigateToNextTab,
      getTab,
      hasTab,
      findTabByWorkingDir,
    }),
    [
      tabs,
      activeTabId,
      addTab,
      removeTab,
      setActiveTab,
      navigateToPreviousTab,
      navigateToNextTab,
      getTab,
      hasTab,
      findTabByWorkingDir,
    ]
  );
}
