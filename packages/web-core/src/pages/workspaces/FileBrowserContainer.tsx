import { Group, Panel, Separator } from 'react-resizable-panels';
import { FileBrowserTreePanel } from './FileBrowserTreePanel';
import { FileBrowserViewerPanel } from './FileBrowserViewerPanel';
import {
  useFileBrowserSource,
  useFileBrowserCurrentPath,
  useFileBrowserSelectedFile,
  useFileBrowserFilterTerm,
  useFileBrowserViewMode,
  useFileBrowserActions,
} from '@/shared/stores/useFileBrowserStore';
import { useDirectoryListing, useFileContent } from '@/shared/hooks/useFileBrowser';

interface FileBrowserContainerProps {
  workspaceId: string;
  className?: string;
}

export function FileBrowserContainer({
  workspaceId,
  className,
}: FileBrowserContainerProps) {
  const source = useFileBrowserSource();
  const currentPath = useFileBrowserCurrentPath();
  const selectedFile = useFileBrowserSelectedFile();
  const filterTerm = useFileBrowserFilterTerm();
  const viewMode = useFileBrowserViewMode();
  const { setSource, navigate, selectFile, setFilterTerm, setViewMode } =
    useFileBrowserActions();

  const { data: listing, isLoading: isListingLoading } = useDirectoryListing(
    workspaceId,
    currentPath,
    source,
  );

  const { data: fileData, isLoading: isFileLoading } = useFileContent(
    workspaceId,
    selectedFile,
    source,
  );

  return (
    <div className={className ?? 'h-full min-h-0'}>
      <Group
        orientation="horizontal"
        className="h-full"
        defaultLayout={{ 'file-browser-tree': 35, 'file-browser-viewer': 65 }}
      >
        <Panel id="file-browser-tree" minSize="20%">
          <FileBrowserTreePanel
            listing={listing}
            isLoading={isListingLoading}
            source={source}
            currentPath={currentPath}
            selectedFile={selectedFile}
            filterTerm={filterTerm}
            onSetSource={setSource}
            onNavigate={navigate}
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
            viewMode={viewMode}
            onSetViewMode={setViewMode}
          />
        </Panel>
      </Group>
    </div>
  );
}
