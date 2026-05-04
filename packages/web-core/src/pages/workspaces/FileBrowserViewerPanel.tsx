import { CopyIcon } from '@phosphor-icons/react';
import { cn } from '@/shared/lib/utils';
import { FileBrowserCodeViewer } from './FileBrowserCodeViewer';
import { FileBrowserMarkdownViewer } from './FileBrowserMarkdownViewer';
import { FileBrowserHtmlViewer } from './FileBrowserHtmlViewer';
import { isMarkdownFile, isHtmlFile } from '@/shared/hooks/useFileBrowser';
import type { FileViewMode } from '@/shared/stores/useFileBrowserStore';
import type { FileContentResponse } from 'shared/types';

interface FileBrowserViewerPanelProps {
  selectedFile: string | null;
  fileData: FileContentResponse | undefined;
  isLoading: boolean;
  viewMode: FileViewMode;
  onSetViewMode: (mode: FileViewMode) => void;
}

export function FileBrowserViewerPanel({
  selectedFile,
  fileData,
  isLoading,
  viewMode,
  onSetViewMode,
}: FileBrowserViewerPanelProps) {
  if (!selectedFile) {
    return (
      <div className="flex-1 flex items-center justify-center text-low text-sm">
        Select a file to view
      </div>
    );
  }

  const isMd = isMarkdownFile(selectedFile);
  const isHtml = isHtmlFile(selectedFile);
  const isBinary = fileData?.content === '__BINARY__';

  return (
    <div className="flex flex-col h-full min-h-0">
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-border shrink-0">
        <span
          className="font-mono text-xs text-low truncate flex-1"
          title={selectedFile}
        >
          {selectedFile}
        </span>

        {isMd && !isBinary && (
          <div className="flex gap-0.5">
            {(['preview', 'raw'] as const).map((m) => (
              <button
                key={m}
                type="button"
                onClick={() => onSetViewMode(m)}
                className={cn(
                  'px-2 py-0.5 text-xs rounded border transition-colors',
                  viewMode === m
                    ? 'bg-secondary border-border text-normal'
                    : 'border-transparent text-low hover:text-normal'
                )}
              >
                {m === 'preview' ? 'Preview' : 'Raw'}
              </button>
            ))}
          </div>
        )}

        {isHtml && !isBinary && (
          <div className="flex gap-0.5">
            {(['rendered', 'source'] as const).map((m) => (
              <button
                key={m}
                type="button"
                onClick={() => onSetViewMode(m)}
                className={cn(
                  'px-2 py-0.5 text-xs rounded border transition-colors',
                  viewMode === m
                    ? 'bg-secondary border-border text-normal'
                    : 'border-transparent text-low hover:text-normal'
                )}
              >
                {m === 'rendered' ? 'Rendered' : 'Source'}
              </button>
            ))}
          </div>
        )}

        <button
          type="button"
          title="Copy path"
          onClick={() => navigator.clipboard.writeText(selectedFile)}
          className="text-low hover:text-normal transition-colors p-0.5"
        >
          <CopyIcon className="size-3.5" />
        </button>
      </div>

      {/* Body */}
      <div className="flex-1 min-h-0 overflow-hidden">
        {isLoading ? (
          <div className="flex items-center justify-center h-full">
            <div className="size-5 animate-spin rounded-full border-2 border-border border-t-brand" />
          </div>
        ) : isBinary ? (
          <div className="flex items-center justify-center h-full text-low text-sm">
            Binary file — cannot display
          </div>
        ) : !fileData ? (
          <div className="flex items-center justify-center h-full text-low text-sm">
            File not found
          </div>
        ) : (
          <div className="flex flex-col h-full min-h-0">
            {fileData.truncated && (
              <div className="px-3 py-1.5 bg-warning/10 text-warning text-xs shrink-0">
                File truncated at 500 KB — showing partial content
              </div>
            )}
            <div className="flex-1 min-h-0 overflow-hidden">
              {renderContent(
                selectedFile,
                fileData.content,
                fileData.language ?? null,
                viewMode
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function renderContent(
  path: string,
  content: string,
  language: string | null,
  viewMode: FileViewMode
) {
  if (isMarkdownFile(path)) {
    return <FileBrowserMarkdownViewer content={content} viewMode={viewMode} />;
  }
  if (isHtmlFile(path)) {
    return <FileBrowserHtmlViewer content={content} viewMode={viewMode} />;
  }
  return <FileBrowserCodeViewer content={content} language={language} />;
}
