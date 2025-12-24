import { describe, it, expect } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useTerminalTabs } from './useTerminalTabs';

describe('useTerminalTabs', () => {
  describe('initial state', () => {
    it('starts with no tabs', () => {
      const { result } = renderHook(() => useTerminalTabs());

      expect(result.current.tabs).toEqual([]);
      expect(result.current.activeTabId).toBeNull();
    });
  });

  describe('addTab', () => {
    it('adds a tab with generated label from path', () => {
      const { result } = renderHook(() => useTerminalTabs());

      act(() => {
        result.current.addTab('/home/user/project');
      });

      expect(result.current.tabs).toHaveLength(1);
      expect(result.current.tabs[0].label).toBe('project');
      expect(result.current.tabs[0].workingDir).toBe('/home/user/project');
    });

    it('adds a tab with custom label', () => {
      const { result } = renderHook(() => useTerminalTabs());

      act(() => {
        result.current.addTab('/some/path', 'My Terminal');
      });

      expect(result.current.tabs[0].label).toBe('My Terminal');
    });

    it('sets new tab as active', () => {
      const { result } = renderHook(() => useTerminalTabs());

      let tabId: string | null = null;
      act(() => {
        tabId = result.current.addTab('/path');
      });

      expect(result.current.activeTabId).toBe(tabId);
    });

    it('returns null when max tabs reached', () => {
      const { result } = renderHook(() => useTerminalTabs({ maxTabs: 2 }));

      act(() => {
        result.current.addTab('/path1');
        result.current.addTab('/path2');
      });

      let thirdTabId: string | null = null;
      act(() => {
        thirdTabId = result.current.addTab('/path3');
      });

      expect(thirdTabId).toBeNull();
      expect(result.current.tabs).toHaveLength(2);
    });

    it('generates unique labels for same directory name', () => {
      const { result } = renderHook(() => useTerminalTabs());

      // Each addTab needs its own act block to allow state to update
      // so the label generator can see the previous tabs
      act(() => {
        result.current.addTab('/path/to/project');
      });
      act(() => {
        result.current.addTab('/other/path/project');
      });
      act(() => {
        result.current.addTab('/third/path/project');
      });

      expect(result.current.tabs[0].label).toBe('project');
      expect(result.current.tabs[1].label).toBe('project (2)');
      expect(result.current.tabs[2].label).toBe('project (3)');
    });
  });

  describe('removeTab', () => {
    it('removes the specified tab', () => {
      const { result } = renderHook(() => useTerminalTabs());

      let tabId: string | null = null;
      act(() => {
        tabId = result.current.addTab('/path');
      });

      act(() => {
        result.current.removeTab(tabId!);
      });

      expect(result.current.tabs).toHaveLength(0);
    });

    it('activates previous tab when removing active tab', () => {
      const { result } = renderHook(() => useTerminalTabs());

      let tab1Id: string | null = null;
      let tab2Id: string | null = null;
      act(() => {
        tab1Id = result.current.addTab('/path1');
        tab2Id = result.current.addTab('/path2');
      });

      // tab2 is active (most recently added)
      expect(result.current.activeTabId).toBe(tab2Id);

      act(() => {
        result.current.removeTab(tab2Id!);
      });

      expect(result.current.activeTabId).toBe(tab1Id);
    });

    it('sets activeTabId to null when last tab removed', () => {
      const { result } = renderHook(() => useTerminalTabs());

      let tabId: string | null = null;
      act(() => {
        tabId = result.current.addTab('/path');
      });

      act(() => {
        result.current.removeTab(tabId!);
      });

      expect(result.current.activeTabId).toBeNull();
    });

    it('does nothing for non-existent tab', () => {
      const { result } = renderHook(() => useTerminalTabs());

      act(() => {
        result.current.addTab('/path');
      });

      act(() => {
        result.current.removeTab('non-existent-id');
      });

      expect(result.current.tabs).toHaveLength(1);
    });
  });

  describe('setActiveTab', () => {
    it('changes active tab', () => {
      const { result } = renderHook(() => useTerminalTabs());

      let tab1Id: string | null = null;
      act(() => {
        tab1Id = result.current.addTab('/path1');
        result.current.addTab('/path2');
      });

      act(() => {
        result.current.setActiveTab(tab1Id!);
      });

      expect(result.current.activeTabId).toBe(tab1Id);
    });
  });

  describe('navigateToPreviousTab', () => {
    it('moves to previous tab', () => {
      const { result } = renderHook(() => useTerminalTabs());

      let tab1Id: string | null = null;
      let tab2Id: string | null = null;
      act(() => {
        tab1Id = result.current.addTab('/path1');
        tab2Id = result.current.addTab('/path2');
      });

      expect(result.current.activeTabId).toBe(tab2Id);

      act(() => {
        result.current.navigateToPreviousTab();
      });

      expect(result.current.activeTabId).toBe(tab1Id);
    });

    it('does nothing at first tab', () => {
      const { result } = renderHook(() => useTerminalTabs());

      let tab1Id: string | null = null;
      act(() => {
        tab1Id = result.current.addTab('/path1');
        result.current.addTab('/path2');
      });

      // Go to first tab
      act(() => {
        result.current.setActiveTab(tab1Id!);
      });

      act(() => {
        result.current.navigateToPreviousTab();
      });

      expect(result.current.activeTabId).toBe(tab1Id);
    });

    it('does nothing with single tab', () => {
      const { result } = renderHook(() => useTerminalTabs());

      let tabId: string | null = null;
      act(() => {
        tabId = result.current.addTab('/path');
      });

      act(() => {
        result.current.navigateToPreviousTab();
      });

      expect(result.current.activeTabId).toBe(tabId);
    });
  });

  describe('navigateToNextTab', () => {
    it('moves to next tab', () => {
      const { result } = renderHook(() => useTerminalTabs());

      let tab1Id: string | null = null;
      let tab2Id: string | null = null;
      act(() => {
        tab1Id = result.current.addTab('/path1');
        tab2Id = result.current.addTab('/path2');
      });

      // Go to first tab
      act(() => {
        result.current.setActiveTab(tab1Id!);
      });

      act(() => {
        result.current.navigateToNextTab();
      });

      expect(result.current.activeTabId).toBe(tab2Id);
    });

    it('does nothing at last tab', () => {
      const { result } = renderHook(() => useTerminalTabs());

      let tab2Id: string | null = null;
      act(() => {
        result.current.addTab('/path1');
        tab2Id = result.current.addTab('/path2');
      });

      // Already at last tab
      act(() => {
        result.current.navigateToNextTab();
      });

      expect(result.current.activeTabId).toBe(tab2Id);
    });
  });

  describe('getTab', () => {
    it('returns tab by id', () => {
      const { result } = renderHook(() => useTerminalTabs());

      let tabId: string | null = null;
      act(() => {
        tabId = result.current.addTab('/path', 'My Terminal');
      });

      const tab = result.current.getTab(tabId!);
      expect(tab?.label).toBe('My Terminal');
      expect(tab?.workingDir).toBe('/path');
    });

    it('returns undefined for non-existent id', () => {
      const { result } = renderHook(() => useTerminalTabs());

      const tab = result.current.getTab('non-existent');
      expect(tab).toBeUndefined();
    });
  });

  describe('hasTab', () => {
    it('returns true for existing working dir', () => {
      const { result } = renderHook(() => useTerminalTabs());

      act(() => {
        result.current.addTab('/my/path');
      });

      expect(result.current.hasTab('/my/path')).toBe(true);
    });

    it('returns false for non-existent working dir', () => {
      const { result } = renderHook(() => useTerminalTabs());

      expect(result.current.hasTab('/my/path')).toBe(false);
    });
  });

  describe('findTabByWorkingDir', () => {
    it('finds tab by working directory', () => {
      const { result } = renderHook(() => useTerminalTabs());

      let tabId: string | null = null;
      act(() => {
        tabId = result.current.addTab('/my/path', 'My Term');
      });

      const tab = result.current.findTabByWorkingDir('/my/path');
      expect(tab?.id).toBe(tabId);
      expect(tab?.label).toBe('My Term');
    });

    it('returns undefined for non-existent directory', () => {
      const { result } = renderHook(() => useTerminalTabs());

      const tab = result.current.findTabByWorkingDir('/non/existent');
      expect(tab).toBeUndefined();
    });
  });
});
