/**
 * File path utility helpers
 */

/**
 * Check if a file path is a markdown file
 * Supports .md, .markdown, and .mdx extensions (case-insensitive)
 */
export function isMarkdownFile(path: string): boolean {
  return /\.(md|markdown|mdx)$/i.test(path);
}
