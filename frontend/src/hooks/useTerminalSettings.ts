import { useState, useEffect, useCallback, useMemo } from 'react';

export interface TerminalSettings {
  /** Terminal font size in pixels */
  fontSize: number;
  /** Terminal font family */
  fontFamily: string;
  /** Enable cursor blinking */
  cursorBlink: boolean;
  /** Scroll sensitivity multiplier */
  scrollSensitivity: number;
}

const DEFAULT_SETTINGS: TerminalSettings = {
  fontSize: 14,
  fontFamily:
    'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace',
  cursorBlink: true,
  scrollSensitivity: 1,
};

const STORAGE_KEY = 'vibe-kanban-terminal-settings';

function loadSettings(): TerminalSettings {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      const parsed = JSON.parse(stored);
      return { ...DEFAULT_SETTINGS, ...parsed };
    }
  } catch (e) {
    console.warn('Failed to load terminal settings from localStorage:', e);
  }
  return DEFAULT_SETTINGS;
}

function saveSettings(settings: TerminalSettings): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch (e) {
    console.warn('Failed to save terminal settings to localStorage:', e);
  }
}

export interface UseTerminalSettingsReturn {
  settings: TerminalSettings;
  updateSettings: (patch: Partial<TerminalSettings>) => void;
  resetSettings: () => void;
}

/**
 * Hook for managing terminal settings.
 * Settings are persisted to localStorage.
 */
export function useTerminalSettings(): UseTerminalSettingsReturn {
  const [settings, setSettings] = useState<TerminalSettings>(loadSettings);

  // Load settings on mount (in case another tab changed them)
  useEffect(() => {
    setSettings(loadSettings());
  }, []);

  const updateSettings = useCallback((patch: Partial<TerminalSettings>) => {
    setSettings((prev) => {
      const next = { ...prev, ...patch };
      saveSettings(next);
      return next;
    });
  }, []);

  const resetSettings = useCallback(() => {
    setSettings(DEFAULT_SETTINGS);
    saveSettings(DEFAULT_SETTINGS);
  }, []);

  return useMemo(
    () => ({
      settings,
      updateSettings,
      resetSettings,
    }),
    [settings, updateSettings, resetSettings]
  );
}

/**
 * Get the current terminal settings without reactivity.
 * Useful for initial terminal configuration.
 */
export function getTerminalSettings(): TerminalSettings {
  return loadSettings();
}
