import { MarkdownPreview } from '@/shared/components/MarkdownPreview';
import { getActualTheme } from '@/shared/lib/theme';
import { useTheme } from '@/shared/hooks/useTheme';
import type { FileViewMode } from '@/shared/stores/useFileBrowserStore';
import { FileBrowserCodeViewer } from './FileBrowserCodeViewer';

interface FileBrowserMarkdownViewerProps {
  content: string;
  viewMode: FileViewMode;
}

export function FileBrowserMarkdownViewer({
  content,
  viewMode,
}: FileBrowserMarkdownViewerProps) {
  const { theme } = useTheme();
  const actualTheme = getActualTheme(theme);

  if (viewMode === 'raw') {
    return <FileBrowserCodeViewer content={content} language="markdown" />;
  }
  return (
    <div className="h-full overflow-auto p-4 prose prose-sm max-w-none dark:prose-invert">
      <MarkdownPreview content={content} theme={actualTheme} allowRawHtml={false} />
    </div>
  );
}
