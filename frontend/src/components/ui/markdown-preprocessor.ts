/**
 * Pre-processes markdown to escape mid-word underscores.
 * Prevents variable_names from being interpreted as italic.
 *
 * Rules:
 * - Escape underscore only if BOTH preceded AND followed by word characters
 * - This prevents snake_case and VK_DATABASE_PATH from becoming italic
 * - Preserves intentional italic: " _text_ " (space before opening underscore)
 * - Preserves _leading underscores (only one side is word char)
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

      // Escape underscores only when BOTH sides have word characters
      // This prevents snake_case from becoming italic
      // Preserves " _italic_ " because space before _ means no word char on left
      return part.replace(/(?<=\w)_(?=\w)/g, '\\_');
    })
    .join('');
}
