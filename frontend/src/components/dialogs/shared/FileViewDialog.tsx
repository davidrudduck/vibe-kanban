import { useState, useCallback, useEffect, useRef } from 'react';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { useQuery } from '@tanstack/react-query';
import { defineModal } from '@/lib/modals';
import { Code, Eye, X, Loader2, AlertCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { fileBrowserApi } from '@/lib/api';
import MarkdownRenderer from '@/components/ui/markdown-renderer';
import FileContentView from '@/components/NormalizedConversation/FileContentView';

export interface FileViewDialogProps {
  /** Full file path for display in title */
  filePath: string;
  /** Relative path within ~/.claude/ for API request */
  relativePath: string;
}

type ViewMode = 'preview' | 'raw';

const FileViewDialogImpl = NiceModal.create<FileViewDialogProps>(
  ({ filePath, relativePath }) => {
    const modal = useModal();
    const containerRef = useRef<HTMLDivElement>(null);
    const [viewMode, setViewMode] = useState<ViewMode>('preview');

    const isMarkdown = /\.(md|markdown|mdx)$/i.test(filePath);

    const {
      data,
      isLoading,
      error,
    } = useQuery({
      queryKey: ['claude-file', relativePath],
      queryFn: () => fileBrowserApi.readClaudeFile(relativePath),
      enabled: modal.visible,
    });

    // Focus container for keyboard events
    useEffect(() => {
      if (modal.visible && containerRef.current) {
        containerRef.current.focus();
      }
    }, [modal.visible]);

    const handleClose = useCallback(() => {
      modal.hide();
    }, [modal]);

    const toggleViewMode = useCallback(() => {
      setViewMode((v) => (v === 'preview' ? 'raw' : 'preview'));
    }, []);

    const handleKeyDown = useCallback(
      (e: React.KeyboardEvent) => {
        switch (e.key) {
          case 'Escape':
            e.preventDefault();
            handleClose();
            break;
          case 'v':
          case 'V':
            if (isMarkdown && data) {
              e.preventDefault();
              toggleViewMode();
            }
            break;
        }
      },
      [handleClose, isMarkdown, data, toggleViewMode]
    );

    const handleBackdropClick = useCallback(
      (e: React.MouseEvent) => {
        if (e.target === e.currentTarget) {
          handleClose();
        }
      },
      [handleClose]
    );

    if (!modal.visible) return null;

    // Extract filename from path for title
    const fileName = filePath.split('/').pop() || filePath;

    return (
      <div
        ref={containerRef}
        className="fixed inset-0 z-[9999] flex items-center justify-center p-4 outline-none"
        role="dialog"
        aria-modal="true"
        aria-label={`File viewer: ${fileName}`}
        tabIndex={0}
        onKeyDown={handleKeyDown}
        onClick={handleBackdropClick}
      >
        {/* Dark overlay */}
        <div className="absolute inset-0 bg-black/80" />

        {/* Dialog container */}
        <div
          className="relative z-10 flex flex-col bg-background border border-border rounded-lg shadow-xl w-full max-w-4xl max-h-[85vh]"
          onClick={(e) => e.stopPropagation()}
        >
          {/* Header */}
          <div className="flex items-center justify-between px-4 py-3 border-b border-border">
            <div className="flex-1 min-w-0 mr-4">
              <h2 className="text-sm font-medium truncate" title={filePath}>
                {fileName}
              </h2>
              <p
                className="text-xs text-muted-foreground truncate"
                title={filePath}
              >
                {filePath}
              </p>
            </div>
            <div className="flex items-center gap-1">
              {/* View mode toggle (only for markdown) */}
              {isMarkdown && data && (
                <TooltipProvider>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-8 w-8"
                        onClick={toggleViewMode}
                        aria-label={
                          viewMode === 'preview' ? 'View raw' : 'View preview'
                        }
                      >
                        {viewMode === 'preview' ? (
                          <Code className="h-4 w-4" />
                        ) : (
                          <Eye className="h-4 w-4" />
                        )}
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>
                      {viewMode === 'preview'
                        ? 'View raw (V)'
                        : 'View preview (V)'}
                    </TooltipContent>
                  </Tooltip>
                </TooltipProvider>
              )}
              <Button
                variant="ghost"
                size="icon"
                className="h-8 w-8"
                onClick={handleClose}
                aria-label="Close (Esc)"
              >
                <X className="h-4 w-4" />
              </Button>
            </div>
          </div>

          {/* Content area */}
          <div className="flex-1 overflow-auto p-4">
            {isLoading && (
              <div className="flex items-center justify-center py-12">
                <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
              </div>
            )}

            {error && (
              <Alert variant="destructive">
                <AlertCircle className="h-4 w-4" />
                <AlertDescription>
                  {error instanceof Error
                    ? error.message
                    : 'Failed to load file'}
                </AlertDescription>
              </Alert>
            )}

            {data && (
              <>
                {viewMode === 'preview' && isMarkdown ? (
                  <MarkdownRenderer
                    content={data.content}
                    className="max-w-none"
                    enableCopyButton
                  />
                ) : (
                  <FileContentView
                    content={data.content}
                    lang={data.language || 'markdown'}
                  />
                )}
              </>
            )}
          </div>
        </div>
      </div>
    );
  }
);

export const FileViewDialog = defineModal<FileViewDialogProps, void>(
  FileViewDialogImpl
);
