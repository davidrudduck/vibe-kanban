import { describe, it, expect, beforeEach, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useTerminalSettings, getTerminalSettings } from './useTerminalSettings';

// Mock localStorage
const localStorageMock = (() => {
  let store: Record<string, string> = {};

  return {
    getItem: vi.fn((key: string) => store[key] ?? null),
    setItem: vi.fn((key: string, value: string) => {
      store[key] = value;
    }),
    removeItem: vi.fn((key: string) => {
      delete store[key];
    }),
    clear: vi.fn(() => {
      store = {};
    }),
  };
})();

Object.defineProperty(window, 'localStorage', {
  value: localStorageMock,
});

describe('useTerminalSettings', () => {
  beforeEach(() => {
    localStorageMock.clear();
    vi.clearAllMocks();
  });

  describe('initial state', () => {
    it('returns default settings when localStorage is empty', () => {
      const { result } = renderHook(() => useTerminalSettings());

      expect(result.current.settings.fontSize).toBe(14);
      expect(result.current.settings.cursorBlink).toBe(true);
      expect(result.current.settings.scrollSensitivity).toBe(1);
      expect(result.current.settings.fontFamily).toContain('monospace');
    });

    it('loads settings from localStorage', () => {
      localStorageMock.setItem(
        'vibe-kanban-terminal-settings',
        JSON.stringify({ fontSize: 18, cursorBlink: false })
      );

      const { result } = renderHook(() => useTerminalSettings());

      expect(result.current.settings.fontSize).toBe(18);
      expect(result.current.settings.cursorBlink).toBe(false);
      // Other settings should be defaults
      expect(result.current.settings.scrollSensitivity).toBe(1);
    });
  });

  describe('updateSettings', () => {
    it('updates a single setting', () => {
      const { result } = renderHook(() => useTerminalSettings());

      act(() => {
        result.current.updateSettings({ fontSize: 16 });
      });

      expect(result.current.settings.fontSize).toBe(16);
    });

    it('updates multiple settings at once', () => {
      const { result } = renderHook(() => useTerminalSettings());

      act(() => {
        result.current.updateSettings({
          fontSize: 12,
          cursorBlink: false,
          scrollSensitivity: 2,
        });
      });

      expect(result.current.settings.fontSize).toBe(12);
      expect(result.current.settings.cursorBlink).toBe(false);
      expect(result.current.settings.scrollSensitivity).toBe(2);
    });

    it('persists settings to localStorage', () => {
      const { result } = renderHook(() => useTerminalSettings());

      act(() => {
        result.current.updateSettings({ fontSize: 20 });
      });

      expect(localStorageMock.setItem).toHaveBeenCalledWith(
        'vibe-kanban-terminal-settings',
        expect.stringContaining('"fontSize":20')
      );
    });

    it('preserves other settings when updating', () => {
      const { result } = renderHook(() => useTerminalSettings());

      const originalFont = result.current.settings.fontFamily;

      act(() => {
        result.current.updateSettings({ fontSize: 18 });
      });

      expect(result.current.settings.fontFamily).toBe(originalFont);
    });
  });

  describe('resetSettings', () => {
    it('resets to default values', () => {
      const { result } = renderHook(() => useTerminalSettings());

      // Change some settings
      act(() => {
        result.current.updateSettings({
          fontSize: 20,
          cursorBlink: false,
        });
      });

      // Reset
      act(() => {
        result.current.resetSettings();
      });

      expect(result.current.settings.fontSize).toBe(14);
      expect(result.current.settings.cursorBlink).toBe(true);
    });

    it('saves default values to localStorage', () => {
      const { result } = renderHook(() => useTerminalSettings());

      act(() => {
        result.current.resetSettings();
      });

      expect(localStorageMock.setItem).toHaveBeenCalledWith(
        'vibe-kanban-terminal-settings',
        expect.stringContaining('"fontSize":14')
      );
    });
  });
});

describe('getTerminalSettings', () => {
  beforeEach(() => {
    localStorageMock.clear();
    vi.clearAllMocks();
  });

  it('returns default settings when localStorage is empty', () => {
    const settings = getTerminalSettings();

    expect(settings.fontSize).toBe(14);
    expect(settings.cursorBlink).toBe(true);
  });

  it('returns stored settings from localStorage', () => {
    localStorageMock.setItem(
      'vibe-kanban-terminal-settings',
      JSON.stringify({ fontSize: 16 })
    );

    const settings = getTerminalSettings();

    expect(settings.fontSize).toBe(16);
  });

  it('handles invalid JSON gracefully', () => {
    localStorageMock.setItem('vibe-kanban-terminal-settings', 'invalid-json');

    const settings = getTerminalSettings();

    // Should return defaults
    expect(settings.fontSize).toBe(14);
  });
});
