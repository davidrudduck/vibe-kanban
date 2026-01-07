import { useState, useCallback, useEffect } from 'react';
import { useQuery } from '@tanstack/react-query';
import { X, Code, Eye, Copy, Check, Loader2, AlertCircle, RefreshCw } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Alert, AlertDescription } from '@/components/ui/alert';
import MarkdownRenderer from '@/components/ui/markdown-renderer';
import FileContentView from '@/components/NormalizedConversation/FileContentView';
import { writeClipboardViaBridge } from '@/vscode/bridge';
import { fileBrowserApi } from '@/lib/api';
import { useFileViewer } from '@/contexts/FileViewerContext';
import { cn } from '@/lib/utils';

/**
 * Desktop/tablet side panel for viewing files.
 * Slides in from the right and takes 45% width.
 * Supports viewing multiple files with a dropdown selector.
 */
export function FileViewerSidePanel() {
  const {
    isOpen,
    files,
    activeFileIndex,
    viewMode,
    closePanel,
    setActiveFile,
    setViewMode,
  } = useFileViewer();

  const [copied, setCopied] = useState(false);

  const activeFile = files[activeFileIndex];
  const fileName = activeFile?.path.split('/').pop() || '';
  const isMarkdown = /\.(md|markdown|mdx)$/i.test(fileName);

  // Fetch file content
  const {
    data: fileData,
    isLoading,
    error,
    refetch,
  } = useQuery({
    queryKey: ['file-viewer', activeFile?.path, activeFile?.relativePath, activeFile?.attemptId],
    queryFn: async () => {
      if (!activeFile) throw new Error('No file selected');

      if (activeFile.relativePath) {
        return fileBrowserApi.readClaudeFile(activeFile.relativePath);
      }
      if (activeFile.attemptId) {
        return fileBrowserApi.readWorktreeFile(activeFile.attemptId, activeFile.path);
      }
      throw new Error('No file source specified');
    },
    enabled: isOpen && !!activeFile,
  });

  const content = fileData?.content || null;

  // Handle escape key
  useEffect(() => {
    if (!isOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        closePanel();
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, closePanel]);

  // Copy content to clipboard
  const handleCopy = useCallback(async () => {
    if (!content) return;
    try {
      await writeClipboardViaBridge(content);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Silently fail - writeClipboardViaBridge handles fallback
    }
  }, [content]);

  if (!isOpen || files.length === 0) {
    return null;
  }

  return (
    <AnimatePresence>
      {isOpen && (
        <motion.div
          className={cn(
            'w-[45%] min-w-[300px] max-w-[600px]',
            'border-l border-border',
            'flex flex-col h-full',
            'bg-background'
          )}
          initial={{ x: '100%', opacity: 0 }}
          animate={{ x: 0, opacity: 1 }}
          exit={{ x: '100%', opacity: 0 }}
          transition={{ type: 'spring', damping: 30, stiffness: 300 }}
        >
          {/* Header */}
          <div className="flex items-center justify-between px-3 py-2 border-b border-border shrink-0">
            <span className="text-sm font-medium">File Preview</span>
            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7"
              onClick={closePanel}
              aria-label="Close panel"
            >
              <X className="h-4 w-4" />
            </Button>
          </div>

          {/* File selector and toolbar */}
          <div className="flex items-center gap-2 px-3 py-2 border-b border-border shrink-0">
            {/* File dropdown */}
            <Select
              value={activeFileIndex.toString()}
              onValueChange={(value) => setActiveFile(parseInt(value, 10))}
            >
              <SelectTrigger className="flex-1 h-8 text-sm">
                <SelectValue placeholder="Select a file" />
              </SelectTrigger>
              <SelectContent>
                {files.map((file, index) => (
                  <SelectItem key={`${file.path}-${index}`} value={index.toString()}>
                    {file.path.split('/').pop()}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>

            {/* View mode toggle - only for markdown files */}
            {isMarkdown && (
              <Button
                variant="ghost"
                size="icon"
                className="h-8 w-8 shrink-0"
                onClick={() => setViewMode(viewMode === 'preview' ? 'raw' : 'preview')}
                aria-label={viewMode === 'preview' ? 'Show raw' : 'Show preview'}
                title={viewMode === 'preview' ? 'Show raw source' : 'Show preview'}
              >
                {viewMode === 'preview' ? (
                  <Code className="h-4 w-4" />
                ) : (
                  <Eye className="h-4 w-4" />
                )}
              </Button>
            )}

            {/* Refresh button */}
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8 shrink-0"
              onClick={() => refetch()}
              disabled={isLoading}
              aria-label="Refresh"
            >
              <RefreshCw className={cn('h-4 w-4', isLoading && 'animate-spin')} />
            </Button>

            {/* Copy button */}
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8 shrink-0"
              onClick={handleCopy}
              disabled={!content}
              aria-label="Copy content"
            >
              {copied ? (
                <Check className="h-4 w-4 text-green-600" />
              ) : (
                <Copy className="h-4 w-4" />
              )}
            </Button>
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
                  {error instanceof Error ? error.message : 'Failed to load file'}
                </AlertDescription>
              </Alert>
            )}

            {content && !isLoading && !error && (
              <>
                {viewMode === 'preview' && isMarkdown ? (
                  <MarkdownRenderer content={content} className="max-w-none" />
                ) : (
                  <FileContentView
                    content={content}
                    lang={getLanguageFromFilename(fileName)}
                  />
                )}
              </>
            )}
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

/**
 * Get language identifier from filename for syntax highlighting.
 */
function getLanguageFromFilename(filename: string): string {
  const ext = filename.split('.').pop()?.toLowerCase();
  const languageMap: Record<string, string> = {
    ts: 'typescript',
    tsx: 'typescript',
    js: 'javascript',
    jsx: 'javascript',
    py: 'python',
    rs: 'rust',
    go: 'go',
    rb: 'ruby',
    java: 'java',
    kt: 'kotlin',
    swift: 'swift',
    cpp: 'cpp',
    c: 'c',
    h: 'c',
    hpp: 'cpp',
    cs: 'csharp',
    php: 'php',
    sh: 'bash',
    bash: 'bash',
    zsh: 'bash',
    json: 'json',
    yaml: 'yaml',
    yml: 'yaml',
    toml: 'toml',
    xml: 'xml',
    html: 'html',
    css: 'css',
    scss: 'scss',
    sass: 'sass',
    less: 'less',
    sql: 'sql',
    md: 'markdown',
    markdown: 'markdown',
    mdx: 'markdown',
  };
  return languageMap[ext || ''] || 'text';
}
