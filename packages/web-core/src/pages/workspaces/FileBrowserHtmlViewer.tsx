import type { FileViewMode } from '@/shared/stores/useFileBrowserStore';
import { FileBrowserCodeViewer } from './FileBrowserCodeViewer';

interface FileBrowserHtmlViewerProps {
  content: string;
  viewMode: FileViewMode;
}

export function FileBrowserHtmlViewer({
  content,
  viewMode,
}: FileBrowserHtmlViewerProps) {
  if (viewMode === 'source') {
    return <FileBrowserCodeViewer content={content} language="xml" />;
  }
  return (
    <iframe
      srcDoc={content}
      sandbox=""
      className="w-full h-full border-0 bg-white"
      title="HTML preview"
    />
  );
}
