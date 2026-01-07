import {
  createContext,
  useContext,
  useState,
  useCallback,
  useMemo,
  ReactNode,
} from 'react';

type ViewMode = 'preview' | 'raw';

interface FileInfo {
  path: string;
  relativePath?: string;
  attemptId?: string;
}

interface FileViewerState {
  isOpen: boolean;
  files: FileInfo[];
  activeFileIndex: number;
  viewMode: ViewMode;
}

interface FileViewerContextValue extends FileViewerState {
  openFile: (file: FileInfo) => void;
  closePanel: () => void;
  setActiveFile: (index: number) => void;
  setViewMode: (mode: ViewMode) => void;
}

const FileViewerContext = createContext<FileViewerContextValue | null>(null);

interface FileViewerProviderProps {
  children: ReactNode;
}

export function FileViewerProvider({ children }: FileViewerProviderProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [files, setFiles] = useState<FileInfo[]>([]);
  const [activeFileIndex, setActiveFileIndex] = useState(0);
  const [viewMode, setViewMode] = useState<ViewMode>('preview');

  const openFile = useCallback((file: FileInfo) => {
    setFiles((prev) => {
      // Check if file is already in the list
      const existingIndex = prev.findIndex(
        (f) =>
          f.path === file.path &&
          f.relativePath === file.relativePath &&
          f.attemptId === file.attemptId
      );

      if (existingIndex >= 0) {
        // File already exists, just focus it
        setActiveFileIndex(existingIndex);
        return prev;
      }

      // Add new file and focus it
      const newFiles = [...prev, file];
      setActiveFileIndex(newFiles.length - 1);
      return newFiles;
    });
    setIsOpen(true);
  }, []);

  const closePanel = useCallback(() => {
    setIsOpen(false);
    // Clear files when closing panel
    setFiles([]);
    setActiveFileIndex(0);
    setViewMode('preview');
  }, []);

  const setActiveFile = useCallback((index: number) => {
    // Ensure index is not negative (upper bound validated in useMemo)
    setActiveFileIndex(Math.max(0, index));
  }, []);

  const handleSetViewMode = useCallback((mode: ViewMode) => {
    setViewMode(mode);
  }, []);

  const value = useMemo<FileViewerContextValue>(
    () => ({
      isOpen,
      files,
      activeFileIndex: Math.min(activeFileIndex, Math.max(0, files.length - 1)),
      viewMode,
      openFile,
      closePanel,
      setActiveFile,
      setViewMode: handleSetViewMode,
    }),
    [
      isOpen,
      files,
      activeFileIndex,
      viewMode,
      openFile,
      closePanel,
      setActiveFile,
      handleSetViewMode,
    ]
  );

  return (
    <FileViewerContext.Provider value={value}>
      {children}
    </FileViewerContext.Provider>
  );
}

export function useFileViewer(): FileViewerContextValue {
  const context = useContext(FileViewerContext);
  if (!context) {
    throw new Error('useFileViewer must be used within a FileViewerProvider');
  }
  return context;
}
