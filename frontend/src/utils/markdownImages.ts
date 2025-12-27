import type { ImageResponse } from 'shared/types';

export function imageToMarkdown(image: ImageResponse): string {
  return `![${image.original_name}](${image.file_path})`;
}

export function appendImageMarkdown(
  prev: string,
  image: ImageResponse
): string {
  const markdownText = imageToMarkdown(image);
  if (prev.trim() === '') return markdownText + '\n';
  const needsNewline = !prev.endsWith('\n');
  return prev + (needsNewline ? '\n' : '') + markdownText + '\n';
}

/**
 * Insert image markdown at a specific cursor position.
 * Returns the new text and the new cursor position (after the inserted markdown).
 */
export function insertImageMarkdownAtPosition(
  text: string,
  image: ImageResponse,
  position: number
): { newText: string; newCursorPosition: number } {
  const markdown = imageToMarkdown(image);
  // Clamp position to valid range
  const safePosition = Math.max(0, Math.min(position, text.length));
  const before = text.slice(0, safePosition);
  const after = text.slice(safePosition);
  const newText = before + markdown + after;
  const newCursorPosition = safePosition + markdown.length;
  return { newText, newCursorPosition };
}
