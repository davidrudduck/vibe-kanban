interface FileBrowserCodeViewerProps {
  content: string;
  language: string | null;
}

export function FileBrowserCodeViewer({
  content,
  language: _language,
}: FileBrowserCodeViewerProps) {
  return (
    <div className="h-full overflow-auto">
      <pre className="p-4 whitespace-pre text-secondary-foreground font-mono text-xs leading-relaxed">
        <code>{content}</code>
      </pre>
    </div>
  );
}
