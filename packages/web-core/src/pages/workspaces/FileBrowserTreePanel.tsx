import { useMemo } from 'react';
import {
  GitBranchIcon,
  FolderIcon,
  MagnifyingGlassIcon,
  WarningCircleIcon,
} from '@phosphor-icons/react';
import { cn } from '@/shared/lib/utils';
import { FileBrowserTreeNode } from '@vibe/ui/components/FileBrowserTreeNode';
import type { DirectoryListResponse } from 'shared/types';
import type { FileSource } from '@/shared/stores/useFileBrowserStore';

interface FileBrowserTreePanelProps {
  listing: DirectoryListResponse | undefined;
  isLoading: boolean;
  isError: boolean;
  source: FileSource;
  currentPath: string | null;
  selectedFile: string | null;
  filterTerm: string;
  onSetSource: (s: FileSource) => void;
  onNavigate: (path: string | null) => void;
  onSelectFile: (path: string) => void;
  onSetFilterTerm: (t: string) => void;
}

export function FileBrowserTreePanel({
  listing,
  isLoading,
  isError,
  source,
  currentPath,
  selectedFile,
  filterTerm,
  onSetSource,
  onNavigate,
  onSelectFile,
  onSetFilterTerm,
}: FileBrowserTreePanelProps) {
  const filteredEntries = useMemo(() => {
    if (!listing) return [];
    const term = filterTerm.toLowerCase();
    const entries = term
      ? listing.entries.filter((e) => e.name.toLowerCase().includes(term))
      : listing.entries;
    // Dirs first, then alphabetical (backend already sorts, but re-sort after filter)
    return [...entries].sort((a, b) =>
      a.is_directory === b.is_directory
        ? a.name.localeCompare(b.name)
        : a.is_directory
          ? -1
          : 1
    );
  }, [listing, filterTerm]);

  const breadcrumbs = useMemo(() => {
    if (!currentPath) return [];
    return currentPath.split('/').filter(Boolean);
  }, [currentPath]);

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

      {/* Breadcrumb */}
      {currentPath && (
        <div className="flex items-center gap-0.5 px-2 py-1 text-xs text-low shrink-0 border-b border-border overflow-x-auto">
          <button
            type="button"
            onClick={() => onNavigate(null)}
            className="hover:text-normal shrink-0"
          >
            root
          </button>
          {breadcrumbs.map((crumb, i) => {
            const path = breadcrumbs.slice(0, i + 1).join('/');
            return (
              <span key={path} className="flex items-center gap-0.5 shrink-0">
                <span className="text-border">/</span>
                <button
                  type="button"
                  onClick={() => onNavigate(path)}
                  className="hover:text-normal"
                >
                  {crumb}
                </button>
              </span>
            );
          })}
        </div>
      )}

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
        ) : isLoading ? (
          <div className="flex items-center justify-center py-8">
            <div className="size-4 animate-spin rounded-full border-2 border-border border-t-brand" />
          </div>
        ) : filteredEntries.length === 0 ? (
          <div className="px-3 py-4 text-xs text-low text-center">
            {filterTerm ? 'No matches' : 'Empty directory'}
          </div>
        ) : (
          filteredEntries.map((entry) => (
            <FileBrowserTreeNode
              key={entry.path}
              entry={entry}
              isSelected={selectedFile === entry.path}
              onClickFolder={(path) => onNavigate(path)}
              onClickFile={(path) => onSelectFile(path)}
            />
          ))
        )}
      </div>
    </div>
  );
}
