import { useQuery } from '@tanstack/react-query';
import { workspacesApi } from '@/shared/lib/api';
import type { FileSource } from '@/shared/stores/useFileBrowserStore';

export const fileBrowserKeys = {
  directory: (id: string, path: string, source: FileSource) =>
    ['workspaceFiles', 'dir', id, source, path] as const,
  file: (id: string, path: string, source: FileSource) =>
    ['workspaceFiles', 'file', id, source, path] as const,
};

export function useDirectoryListing(
  workspaceId: string | undefined,
  path: string | null,
  source: FileSource
) {
  return useQuery({
    queryKey: fileBrowserKeys.directory(workspaceId ?? '', path ?? '', source),
    queryFn: () => workspacesApi.listFiles(workspaceId!, path ?? '', source),
    enabled: !!workspaceId,
    staleTime: 30_000,
  });
}

export function useFileContent(
  workspaceId: string | undefined,
  filePath: string | null,
  source: FileSource
) {
  return useQuery({
    queryKey: fileBrowserKeys.file(workspaceId ?? '', filePath ?? '', source),
    queryFn: () =>
      workspacesApi.getFileContent(workspaceId!, filePath!, source),
    enabled: !!workspaceId && !!filePath,
    staleTime: 60_000,
  });
}

export function isMarkdownFile(path: string): boolean {
  return /\.(md|markdown|mdx)$/i.test(path);
}

export function isHtmlFile(path: string): boolean {
  return /\.(html|htm)$/i.test(path);
}
