import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ThemeMode } from 'shared/types';
import { ThemeProviderContext } from '@/shared/hooks/useTheme';

export const THEME_STORAGE_KEY = 'vk-theme';

type ThemeProviderProps = {
  children: React.ReactNode;
  /**
   * Server-persisted theme (e.g., from user config). Used only as the initial
   * value when localStorage is empty — once the user has explicitly chosen a
   * theme (in this app or a previous session), localStorage wins and later
   * changes to this prop are ignored. This prevents per-host config refetches
   * from clobbering the user's choice mid-session.
   */
  initialTheme?: ThemeMode;
};

function isValidThemeMode(v: unknown): v is ThemeMode {
  return v === ThemeMode.LIGHT || v === ThemeMode.DARK || v === ThemeMode.SYSTEM;
}

function readStoredTheme(): ThemeMode | null {
  try {
    const stored = localStorage.getItem(THEME_STORAGE_KEY);
    return isValidThemeMode(stored) ? stored : null;
  } catch {
    return null;
  }
}

export function ThemeProvider({
  children,
  initialTheme = ThemeMode.SYSTEM,
}: ThemeProviderProps) {
  const [theme, setThemeState] = useState<ThemeMode>(
    () => readStoredTheme() ?? initialTheme,
  );

  // Tracks whether the user has explicitly chosen a theme. Once true, server
  // config is no longer allowed to override local state.
  const userHasChosen = useRef<boolean>(readStoredTheme() !== null);

  // Soft sync from server config: applies at most once for a fresh install
  // where config arrives async after first render. We lock userHasChosen on
  // the first applied sync so subsequent config changes (e.g. host switches
  // with different saved themes) cannot cause the theme to oscillate.
  useEffect(() => {
    if (userHasChosen.current) return;
    if (initialTheme === theme) return;
    setThemeState(initialTheme);
    userHasChosen.current = true;
  }, [initialTheme, theme]);

  // Cross-tab sync: when another tab writes a new theme, mirror it here.
  // We don't flip userHasChosen because the originating tab already did.
  useEffect(() => {
    const onStorage = (e: StorageEvent) => {
      if (e.key !== THEME_STORAGE_KEY) return;
      if (!isValidThemeMode(e.newValue)) return;
      setThemeState(e.newValue);
    };
    window.addEventListener('storage', onStorage);
    return () => window.removeEventListener('storage', onStorage);
  }, []);

  // Apply class to <html>; subscribe to OS prefers-color-scheme when SYSTEM.
  useEffect(() => {
    const root = document.documentElement;
    const apply = (mode: 'light' | 'dark') => {
      root.classList.remove('light', 'dark');
      root.classList.add(mode);
    };

    if (theme === ThemeMode.SYSTEM) {
      const media = window.matchMedia('(prefers-color-scheme: dark)');
      apply(media.matches ? 'dark' : 'light');
      const onChange = (e: MediaQueryListEvent) => apply(e.matches ? 'dark' : 'light');
      media.addEventListener('change', onChange);
      return () => media.removeEventListener('change', onChange);
    }

    apply(theme === ThemeMode.DARK ? 'dark' : 'light');
  }, [theme]);

  const setTheme = useCallback((next: ThemeMode) => {
    // Defense-in-depth: callers from outside this module (e.g. postMessage
    // bridges) may pass unvalidated values. Reject anything not in the enum
    // to keep localStorage and the <html> class in a consistent state.
    if (!isValidThemeMode(next)) return;
    userHasChosen.current = true;
    try {
      localStorage.setItem(THEME_STORAGE_KEY, next);
    } catch {
      // localStorage may be blocked (private mode, quota); state still updates.
    }
    setThemeState(next);
  }, []);

  const value = useMemo(() => ({ theme, setTheme }), [theme, setTheme]);

  return (
    <ThemeProviderContext.Provider value={value}>
      {children}
    </ThemeProviderContext.Provider>
  );
}
