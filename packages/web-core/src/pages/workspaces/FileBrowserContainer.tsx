import { useEffect, useRef } from 'react';
import { Group, Panel, Separator } from 'react-resizable-panels';
import { FileBrowserTreePanel } from './FileBrowserTreePanel';
import { FileBrowserViewerPanel } from './FileBrowserViewerPanel';
import {
  useFileBrowserSource,
  useFileBrowserSelectedFile,
  useFileBrowserFilterTerm,
  useFileBrowserViewMode,
  useFileBrowserExpandedFolderPaths,
  useFileBrowserOpenFileWorkspaceId,
  useFileBrowserActions,
} from '@/shared/stores/useFileBrowserStore';
import {
  useFileContent,
  useDirectoryListing,
} from '@/shared/hooks/useFileBrowser';

interface FileBrowserContainerProps {
  workspaceId: string;
  className?: string;
}

export function FileBrowserContainer({
  workspaceId,
  className,
}: FileBrowserContainerProps) {
  const source = useFileBrowserSource();
  const selectedFile = useFileBrowserSelectedFile();
  const filterTerm = useFileBrowserFilterTerm();
  const viewMode = useFileBrowserViewMode();
  const expandedFolderPaths = useFileBrowserExpandedFolderPaths();
  const openFileWorkspaceId = useFileBrowserOpenFileWorkspaceId();
  const {
    setSource,
    selectFile,
    setFilterTerm,
    setViewMode,
    resetForWorkspace,
    toggleFolderExpanded,
    consumeOpenFileIntent,
  } = useFileBrowserActions();

  // Tracks whether this component instance has completed its first mount.
  const hasMountedRef = useRef(false);
  // Tracks the workspaceId seen on the previous render, for change detection.
  const lastWorkspaceIdRef = useRef(workspaceId);

  useEffect(() => {
    if (!hasMountedRef.current) {
      // First mount of this instance.
      hasMountedRef.current = true;
      lastWorkspaceIdRef.current = workspaceId;

      if (openFileWorkspaceId === workspaceId) {
        // openFile() was called for THIS workspace just before the panel
        // mounted — preserve the pending selectedFile/currentPath state,
        // then clear the intent so future remounts start fresh.
        consumeOpenFileIntent();
        return;
      }
      // No pending openFile() for this workspace — start with a clean slate.
      resetForWorkspace();
      return;
    }

    // Subsequent renders: reset only when the workspace actually changes.
    if (lastWorkspaceIdRef.current !== workspaceId) {
      lastWorkspaceIdRef.current = workspaceId;
      resetForWorkspace();
    }
  }, [
    workspaceId,
    openFileWorkspaceId,
    resetForWorkspace,
    consumeOpenFileIntent,
  ]);

  // Root listing is only needed to surface top-level errors in the tree panel
  const { isError: isRootError } = useDirectoryListing(workspaceId, '', source);

  const {
    data: fileData,
    isLoading: isFileLoading,
    isError: isFileError,
  } = useFileContent(workspaceId, selectedFile, source);

  return (
    <div className={className ?? 'h-full min-h-0'}>
      <Group
        orientation="horizontal"
        className="h-full"
        defaultLayout={{ 'file-browser-tree': 35, 'file-browser-viewer': 65 }}
      >
        <Panel id="file-browser-tree" minSize="20%">
          <FileBrowserTreePanel
            workspaceId={workspaceId}
            source={source}
            selectedFile={selectedFile}
            filterTerm={filterTerm}
            expandedFolderPaths={expandedFolderPaths}
            isError={isRootError}
            onSetSource={setSource}
            onToggleFolder={toggleFolderExpanded}
            onSelectFile={(path) => selectFile(path)}
            onSetFilterTerm={setFilterTerm}
          />
        </Panel>

        <Separator
          id="file-browser-separator"
          className="w-1 bg-border hover:bg-brand/50 transition-colors cursor-col-resize"
        />

        <Panel id="file-browser-viewer" minSize="30%">
          <FileBrowserViewerPanel
            selectedFile={selectedFile}
            fileData={fileData}
            isLoading={isFileLoading}
            isError={isFileError}
            viewMode={viewMode}
            onSetViewMode={setViewMode}
          />
        </Panel>
      </Group>
    </div>
  );
}
