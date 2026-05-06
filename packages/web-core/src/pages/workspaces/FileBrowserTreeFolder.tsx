import { useMemo } from 'react';
import { FileBrowserTreeNode } from '@vibe/ui/components/FileBrowserTreeNode';
import { useDirectoryListing } from '@/shared/hooks/useFileBrowser';
import type { FileSource } from '@/shared/stores/useFileBrowserStore';

interface FileBrowserTreeFolderProps {
  workspaceId: string;
  path: string; // '' = root
  source: FileSource;
  depth: number;
  expandedPaths: Set<string>;
  selectedFile: string | null;
  filterTerm: string;
  onToggleFolder: (path: string) => void;
  onSelectFile: (path: string) => void;
}

export function FileBrowserTreeFolder({
  workspaceId,
  path,
  source,
  depth,
  expandedPaths,
  selectedFile,
  filterTerm,
  onToggleFolder,
  onSelectFile,
}: FileBrowserTreeFolderProps) {
  const { data: listing, isLoading } = useDirectoryListing(
    workspaceId,
    path,
    source
  );

  const entries = useMemo(() => {
    if (!listing) return [];
    const term = filterTerm.toLowerCase();
    const filtered = term
      ? listing.entries.filter((e) => e.name.toLowerCase().includes(term))
      : listing.entries;
    return [...filtered].sort((a, b) =>
      a.is_directory === b.is_directory
        ? a.name.localeCompare(b.name)
        : a.is_directory
          ? -1
          : 1
    );
  }, [listing, filterTerm]);

  if (isLoading && depth === 0) {
    return (
      <div className="flex items-center justify-center py-8">
        <div className="size-4 animate-spin rounded-full border-2 border-border border-t-brand" />
      </div>
    );
  }

  return (
    <>
      {entries.map((entry) => {
        const isExpanded = entry.is_directory && expandedPaths.has(entry.path);
        return (
          <div key={entry.path}>
            <FileBrowserTreeNode
              entry={entry}
              depth={depth}
              isExpanded={isExpanded}
              isSelected={selectedFile === entry.path}
              onClickFolder={onToggleFolder}
              onClickFile={onSelectFile}
            />
            {isExpanded && (
              <FileBrowserTreeFolder
                workspaceId={workspaceId}
                path={entry.path}
                source={source}
                depth={depth + 1}
                expandedPaths={expandedPaths}
                selectedFile={selectedFile}
                filterTerm={filterTerm}
                onToggleFolder={onToggleFolder}
                onSelectFile={onSelectFile}
              />
            )}
          </div>
        );
      })}
    </>
  );
}
