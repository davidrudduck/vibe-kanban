import { useState, useCallback, useRef, useEffect } from 'react';
import {
  motion,
  AnimatePresence,
  PanInfo,
  useDragControls,
} from 'framer-motion';
import { ArrowLeft, X, Code, Eye, Copy, Check, Loader2, AlertCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription } from '@/components/ui/alert';
import MarkdownRenderer from '@/components/ui/markdown-renderer';
import FileContentView from '@/components/NormalizedConversation/FileContentView';
import { writeClipboardViaBridge } from '@/vscode/bridge';
import { cn } from '@/lib/utils';

/** Minimum drag distance in pixels required to close the sheet */
const DRAG_CLOSE_THRESHOLD = 100;

/**
 * Display mode for file content.
 * - 'preview': Rendered markdown with formatting
 * - 'raw': Plain text source view
 */
type ViewMode = 'preview' | 'raw';

/**
 * Props for the FileViewerSheet component.
 */
export interface FileViewerSheetProps {
  /** Whether the sheet is open/visible */
  open: boolean;
  /** Callback fired when the sheet should close */
  onClose: () => void;
  /** Name of the file being displayed (used for title and language detection) */
  fileName: string;
  /** File content to display, or null if not yet loaded */
  content: string | null;
  /** Language identifier for syntax highlighting (defaults to detection from fileName) */
  language?: string;
  /** Whether file content is currently loading */
  isLoading?: boolean;
  /** Error that occurred while loading the file, if any */
  error?: Error | null;
}

/**
 * Full-screen mobile file viewer sheet component.
 * Slides up from bottom of the screen with swipe-to-close gesture support.
 *
 * Features:
 * - Full viewport height (100dvh) for maximum content visibility
 * - Drag handle at top for intuitive swipe-to-close gesture
 * - Toggle between rendered markdown preview and raw source view
 * - Copy to clipboard functionality
 * - Loading and error states
 * - Escape key to close
 * - Backdrop click to close
 *
 * @example
 * ```tsx
 * <FileViewerSheet
 *   open={isOpen}
 *   onClose={() => setIsOpen(false)}
 *   fileName="README.md"
 *   content={fileContent}
 *   isLoading={isLoading}
 *   error={error}
 * />
 * ```
 */
export function FileViewerSheet({
  open,
  onClose,
  fileName,
  content,
  language,
  isLoading = false,
  error = null,
}: FileViewerSheetProps) {
  const [viewMode, setViewMode] = useState<ViewMode>('preview');
  const [copied, setCopied] = useState(false);
  const dragControls = useDragControls();
  const sheetRef = useRef<HTMLDivElement>(null);

  const isMarkdown = /\.(md|markdown|mdx)$/i.test(fileName);

  // Handle drag end to determine if should close
  const handleDragEnd = useCallback(
    (_event: MouseEvent | TouchEvent | PointerEvent, info: PanInfo) => {
      if (info.offset.y > DRAG_CLOSE_THRESHOLD || info.velocity.y > 500) {
        onClose();
      }
    },
    [onClose]
  );

  // Handle backdrop click
  const handleBackdropClick = useCallback(
    (e: React.MouseEvent) => {
      if (e.target === e.currentTarget) {
        onClose();
      }
    },
    [onClose]
  );

  // Handle escape key
  useEffect(() => {
    if (!open) return;

    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };

    document.addEventListener('keydown', handleEscape);
    return () => document.removeEventListener('keydown', handleEscape);
  }, [open, onClose]);

  // Prevent body scroll when sheet is open
  useEffect(() => {
    if (!open) return;

    const originalOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    return () => {
      document.body.style.overflow = originalOverflow;
    };
  }, [open]);


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

  return (
    <AnimatePresence>
      {open && (
        <>
          {/* Backdrop */}
          <motion.div
            className="fixed inset-0 z-50 bg-black/50"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            onClick={handleBackdropClick}
          />

          {/* Sheet */}
          <motion.div
            ref={sheetRef}
            className={cn(
              'fixed bottom-0 left-0 right-0 z-50',
              'bg-background text-foreground',
              'h-[100dvh] flex flex-col'
            )}
            initial={{ y: '100%' }}
            animate={{ y: 0 }}
            exit={{ y: '100%' }}
            transition={{ type: 'spring', damping: 30, stiffness: 300 }}
            drag="y"
            dragControls={dragControls}
            dragConstraints={{ top: 0, bottom: 0 }}
            dragElastic={{ top: 0, bottom: 0.5 }}
            onDragEnd={handleDragEnd}
          >
            {/* Drag handle */}
            <div
              className="flex justify-center py-2 cursor-grab active:cursor-grabbing touch-none bg-background"
              onPointerDown={(e) => dragControls.start(e)}
            >
              <div className="w-10 h-1 bg-muted-foreground/30 rounded-full" />
            </div>

            {/* Header */}
            <div className="flex items-center justify-between px-4 py-2 border-b border-border">
              <Button
                variant="ghost"
                size="icon"
                className="h-8 w-8"
                onClick={onClose}
                aria-label="Close"
              >
                <ArrowLeft className="h-4 w-4" />
              </Button>
              <span className="font-medium text-sm truncate flex-1 mx-2 text-center">
                {fileName}
              </span>
              <Button
                variant="ghost"
                size="icon"
                className="h-8 w-8"
                onClick={onClose}
                aria-label="Close"
              >
                <X className="h-4 w-4" />
              </Button>
            </div>

            {/* Content */}
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
                    {error.message || 'Failed to load file'}
                  </AlertDescription>
                </Alert>
              )}

              {content && !isLoading && !error && (
                <>
                  {viewMode === 'preview' && isMarkdown ? (
                    <MarkdownRenderer
                      content={content}
                      className="max-w-none"
                    />
                  ) : (
                    <FileContentView
                      content={content}
                      lang={language || 'markdown'}
                    />
                  )}
                </>
              )}
            </div>

            {/* Footer toolbar */}
            <div className="flex items-center justify-center gap-2 px-4 py-3 border-t border-border bg-background">
              {isMarkdown && content && (
                <>
                  <Button
                    variant={viewMode === 'raw' ? 'default' : 'outline'}
                    size="sm"
                    onClick={() => setViewMode('raw')}
                  >
                    <Code className="h-4 w-4 mr-1.5" />
                    Raw
                  </Button>
                  <Button
                    variant={viewMode === 'preview' ? 'default' : 'outline'}
                    size="sm"
                    onClick={() => setViewMode('preview')}
                  >
                    <Eye className="h-4 w-4 mr-1.5" />
                    Preview
                  </Button>
                </>
              )}
              <Button
                variant="outline"
                size="sm"
                onClick={handleCopy}
                disabled={!content}
              >
                {copied ? (
                  <>
                    <Check className="h-4 w-4 mr-1.5 text-green-600" />
                    Copied!
                  </>
                ) : (
                  <>
                    <Copy className="h-4 w-4 mr-1.5" />
                    Copy
                  </>
                )}
              </Button>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}
