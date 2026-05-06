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
  setSource: (source: FileSource) => void;
  navigate: (path: string | null) => void;
  selectFile: (path: string | null, viewMode?: FileViewMode) => void;
  setFilterTerm: (term: string) => void;
  setViewMode: (mode: FileViewMode) => void;
  openFile: (path: string, workspaceId: string) => void;
  /** Clear the pending openFile intent without resetting other state. */
  consumeOpenFileIntent: () => void;
  resetForWorkspace: () => void;
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

  setSource: (source) =>
    set({ source, currentPath: null, selectedFile: null, filterTerm: '' }),

  navigate: (path) =>
    set({ currentPath: path, selectedFile: null, filterTerm: '' }),

  selectFile: (path, viewMode) =>
    set({ selectedFile: path, viewMode: viewMode ?? null }),

  setFilterTerm: (filterTerm) => set({ filterTerm }),

  setViewMode: (viewMode) => set({ viewMode }),

  openFile: (path, workspaceId) => {
    const lastSlash = path.lastIndexOf('/');
    const parentPath = lastSlash > 0 ? path.slice(0, lastSlash) : null;
    // Does NOT override source — preserves user's current worktree/main selection.
    // Stores workspaceId so FileBrowserContainer can detect a pending intent on mount.
    set({
      currentPath: parentPath,
      selectedFile: path,
      viewMode: autoViewMode(path),
      filterTerm: '',
      openFileWorkspaceId: workspaceId,
    });
  },

  consumeOpenFileIntent: () => set({ openFileWorkspaceId: null }),

  resetForWorkspace: () =>
    set({
      source: 'worktree',
      currentPath: null,
      selectedFile: null,
      filterTerm: '',
      viewMode: null,
      openFileWorkspaceId: null,
    }),
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

export const useFileBrowserActions = () =>
  useFileBrowserStore(
    useShallow((s) => ({
      setSource: s.setSource,
      navigate: s.navigate,
      selectFile: s.selectFile,
      setFilterTerm: s.setFilterTerm,
      setViewMode: s.setViewMode,
      openFile: s.openFile,
      consumeOpenFileIntent: s.consumeOpenFileIntent,
      resetForWorkspace: s.resetForWorkspace,
    }))
  );
