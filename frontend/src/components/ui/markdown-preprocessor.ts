/**
 * Pre-processes markdown to escape mid-word underscores.
 * Prevents variable_names from being interpreted as italic.
 *
 * Rules:
 * - Escape underscore if preceded by OR followed by a word character
 * - This prevents snake_case and VK_DATABASE_PATH from becoming italic
 * - Preserves intentional italic: " _text_ " (spaces around both underscores)
 * - Preserve underscores inside backticks (inline code)
 * - Preserve underscores inside fenced code blocks
 * - Preserve underscores in URLs/paths
 */
export function preprocessMarkdown(content: string): string {
  if (!content) return content;

  // Split by code blocks and URLs to avoid modifying them
  // Captures: fenced code, inline code, and URLs
  const parts = content.split(/(```[\s\S]*?```|`[^`]+`|https?:\/\/[^\s)>\]]+)/);

  return parts
    .map((part, index) => {
      // Odd indices are code blocks/inline code/URLs - don't modify
      if (index % 2 === 1) return part;

      // Escape underscores adjacent to word characters on EITHER side
      // This prevents snake_case from becoming italic
      // Preserves " _italic_ " (spaces around both underscores)
      return part.replace(/(?<=\w)_|_(?=\w)/g, '\\_');
    })
    .join('');
}
