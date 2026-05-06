import { create } from 'zustand';
import { useShallow } from 'zustand/react/shallow';

export type FileSource = 'worktree' | 'main';
export type FileViewMode = 'preview' | 'raw' | 'rendered' | 'source' | null;

type FileBrowserState = {
  source: FileSource;
  currentPath: string | null;
  selectedFile: string | null;
  filterTerm: string;
  viewMode: FileViewMode;
  /**
   * Tracks which workspaceId last called openFile(). FileBrowserContainer uses
   * this on first mount to decide whether to preserve the pending selection
   * (same workspace → skip reset) or clear it (different workspace → reset).
   */
  openFileWorkspaceId: string | null;
  /** Set of folder paths currently expanded in the tree panel. */
  expandedFolderPaths: Set<string>;
  setSource: (source: FileSource) => void;
  navigate: (path: string | null) => void;
  selectFile: (path: string | null, viewMode?: FileViewMode) => void;
  setFilterTerm: (term: string) => void;
  setViewMode: (mode: FileViewMode) => void;
  openFile: (path: string, workspaceId: string) => void;
  resetForWorkspace: () => void;
  toggleFolderExpanded: (path: string) => void;
  collapseAllFolders: () => void;
};

function autoViewMode(path: string): FileViewMode {
  const lower = path.toLowerCase();
  if (
    lower.endsWith('.md') ||
    lower.endsWith('.markdown') ||
    lower.endsWith('.mdx')
  ) {
    return 'preview';
  }
  if (lower.endsWith('.html') || lower.endsWith('.htm')) {
    return 'rendered';
  }
  return null;
}

export const useFileBrowserStore = create<FileBrowserState>()((set) => ({
  source: 'worktree',
  currentPath: null,
  selectedFile: null,
  filterTerm: '',
  viewMode: null,
  openFileWorkspaceId: null,
  expandedFolderPaths: new Set<string>(),

  setSource: (source) =>
    set({
      source,
      currentPath: null,
      selectedFile: null,
      filterTerm: '',
      expandedFolderPaths: new Set(),
    }),

  navigate: (path) =>
    set({ currentPath: path, selectedFile: null, filterTerm: '' }),

  selectFile: (path, viewMode) =>
    set({
      selectedFile: path,
      viewMode:
        viewMode !== undefined ? viewMode : path ? autoViewMode(path) : null,
    }),

  setFilterTerm: (filterTerm) => set({ filterTerm }),

  setViewMode: (viewMode) => set({ viewMode }),

  openFile: (path, workspaceId) => {
    // Expand all parent folders so the file is visible in the tree
    const parts = path.split('/').filter(Boolean);
    const parentPaths = parts
      .slice(0, -1)
      .map((_, i) => parts.slice(0, i + 1).join('/'));
    const lastSlash = path.lastIndexOf('/');
    const parentPath = lastSlash > 0 ? path.slice(0, lastSlash) : null;
    set((s) => {
      const next = new Set(s.expandedFolderPaths);
      for (const p of parentPaths) next.add(p);
      return {
        currentPath: parentPath,
        selectedFile: path,
        viewMode: autoViewMode(path),
        filterTerm: '',
        // Stores workspaceId so FileBrowserContainer can detect a pending
        // intent on mount and skip the reset for this workspace.
        openFileWorkspaceId: workspaceId,
        expandedFolderPaths: next,
      };
    });
  },

  resetForWorkspace: () =>
    set({
      source: 'worktree',
      currentPath: null,
      selectedFile: null,
      filterTerm: '',
      viewMode: null,
      openFileWorkspaceId: null,
      expandedFolderPaths: new Set(),
    }),

  toggleFolderExpanded: (path) =>
    set((s) => {
      const next = new Set(s.expandedFolderPaths);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return { expandedFolderPaths: next };
    }),

  collapseAllFolders: () => set({ expandedFolderPaths: new Set() }),
}));

export const useFileBrowserSource = () => useFileBrowserStore((s) => s.source);
export const useFileBrowserCurrentPath = () =>
  useFileBrowserStore((s) => s.currentPath);
export const useFileBrowserSelectedFile = () =>
  useFileBrowserStore((s) => s.selectedFile);
export const useFileBrowserFilterTerm = () =>
  useFileBrowserStore((s) => s.filterTerm);
export const useFileBrowserViewMode = () =>
  useFileBrowserStore((s) => s.viewMode);
export const useFileBrowserOpenFileWorkspaceId = () =>
  useFileBrowserStore((s) => s.openFileWorkspaceId);
export const useFileBrowserExpandedFolderPaths = () =>
  useFileBrowserStore((s) => s.expandedFolderPaths);

export const useFileBrowserActions = () =>
  useFileBrowserStore(
    useShallow((s) => ({
      setSource: s.setSource,
      navigate: s.navigate,
      selectFile: s.selectFile,
      setFilterTerm: s.setFilterTerm,
      setViewMode: s.setViewMode,
      openFile: s.openFile,
      resetForWorkspace: s.resetForWorkspace,
      toggleFolderExpanded: s.toggleFolderExpanded,
      collapseAllFolders: s.collapseAllFolders,
    }))
  );
