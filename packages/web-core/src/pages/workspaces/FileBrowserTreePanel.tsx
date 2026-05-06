import {
  GitBranchIcon,
  FolderIcon,
  MagnifyingGlassIcon,
  WarningCircleIcon,
} from '@phosphor-icons/react';
import { cn } from '@/shared/lib/utils';
import { FileBrowserTreeFolder } from './FileBrowserTreeFolder';
import type { FileSource } from '@/shared/stores/useFileBrowserStore';

interface FileBrowserTreePanelProps {
  workspaceId: string;
  source: FileSource;
  selectedFile: string | null;
  filterTerm: string;
  expandedFolderPaths: Set<string>;
  isError: boolean;
  onSetSource: (s: FileSource) => void;
  onToggleFolder: (path: string) => void;
  onSelectFile: (path: string) => void;
  onSetFilterTerm: (t: string) => void;
}

export function FileBrowserTreePanel({
  workspaceId,
  source,
  selectedFile,
  filterTerm,
  expandedFolderPaths,
  isError,
  onSetSource,
  onToggleFolder,
  onSelectFile,
  onSetFilterTerm,
}: FileBrowserTreePanelProps) {
  return (
    <div className="flex flex-col h-full min-h-0 border-r border-border">
      {/* Source toggle */}
      <div className="flex gap-1 p-2 shrink-0 border-b border-border">
        <button
          type="button"
          onClick={() => onSetSource('worktree')}
          className={cn(
            'flex-1 flex items-center justify-center gap-1 py-1 text-xs rounded transition-colors',
            source === 'worktree'
              ? 'bg-brand text-white'
              : 'bg-secondary text-low hover:text-normal'
          )}
        >
          <GitBranchIcon className="size-3" />
          Worktree
        </button>
        <button
          type="button"
          onClick={() => onSetSource('main')}
          className={cn(
            'flex-1 flex items-center justify-center gap-1 py-1 text-xs rounded transition-colors',
            source === 'main'
              ? 'bg-brand text-white'
              : 'bg-secondary text-low hover:text-normal'
          )}
        >
          <FolderIcon className="size-3" />
          Main
        </button>
      </div>

      {/* Filter */}
      <div className="px-2 py-1.5 shrink-0 border-b border-border">
        <div className="flex items-center gap-1.5 bg-secondary rounded px-2 py-1">
          <MagnifyingGlassIcon className="size-3 text-low shrink-0" />
          <input
            type="text"
            placeholder="Filter files…"
            value={filterTerm}
            onChange={(e) => onSetFilterTerm(e.target.value)}
            className="bg-transparent text-xs outline-none flex-1 text-normal placeholder:text-low"
          />
        </div>
      </div>

      {/* Tree */}
      <div className="flex-1 overflow-y-auto py-1">
        {isError ? (
          <div className="flex flex-col items-center justify-center py-8 gap-2 text-destructive">
            <WarningCircleIcon className="size-5" />
            <span className="text-xs">Failed to load directory</span>
          </div>
        ) : (
          <FileBrowserTreeFolder
            workspaceId={workspaceId}
            path=""
            source={source}
            depth={0}
            expandedPaths={expandedFolderPaths}
            selectedFile={selectedFile}
            filterTerm={filterTerm}
            onToggleFolder={onToggleFolder}
            onSelectFile={onSelectFile}
          />
        )}
      </div>
    </div>
  );
}
