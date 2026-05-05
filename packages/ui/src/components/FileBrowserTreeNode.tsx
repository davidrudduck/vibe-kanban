import {
  CaretDownIcon,
  CaretRightIcon,
  FileIcon,
  FolderIcon,
} from '@phosphor-icons/react';

import { cn } from '../lib/cn';
import type { DirectoryEntry } from 'shared/types';

interface FileBrowserTreeNodeProps {
  entry: DirectoryEntry;
  depth?: number;
  isExpanded?: boolean;
  isSelected?: boolean;
  onClickFolder: (path: string) => void;
  onClickFile: (path: string) => void;
}

export function FileBrowserTreeNode({
  entry,
  depth = 0,
  isExpanded = false,
  isSelected = false,
  onClickFolder,
  onClickFile,
}: FileBrowserTreeNodeProps) {
  const indent = depth * 12;

  return (
    <button
      type="button"
      className={cn(
        'w-full flex items-center gap-1.5 px-2 py-1 text-left text-sm min-h-[32px] rounded-sm',
        'text-secondary-foreground hover:bg-secondary transition-colors',
        isSelected && 'bg-brand/10 text-brand'
      )}
      style={{ paddingLeft: `${8 + indent}px` }}
      onClick={() =>
        entry.is_directory ? onClickFolder(entry.path) : onClickFile(entry.path)
      }
    >
      {entry.is_directory ? (
        <>
          {isExpanded ? (
            <CaretDownIcon className="size-3 shrink-0 text-low" />
          ) : (
            <CaretRightIcon className="size-3 shrink-0 text-low" />
          )}
          <FolderIcon className="size-3.5 shrink-0 text-low" />
        </>
      ) : (
        <>
          <span className="w-3 shrink-0" />
          <FileIcon className="size-3.5 shrink-0 text-low" />
        </>
      )}
      <span className="truncate font-mono text-xs">{entry.name}</span>
    </button>
  );
}
