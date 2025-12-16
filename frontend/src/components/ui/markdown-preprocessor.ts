/**
 * Pre-processes markdown to escape mid-word underscores.
 * Prevents variable_names from being interpreted as italic.
 *
 * Rules:
 * - Escape underscore if preceded by a word character (not whitespace/start)
 * - Preserve underscores inside backticks (inline code)
 * - Preserve underscores inside fenced code blocks
 * - Preserve underscores in URLs/paths (after / or \)
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

      // Escape underscores that are preceded by a word character
      // This prevents snake_case from becoming italic
      // But preserves " _italic_" (space before underscore)
      return part.replace(/(\w)_/g, '$1\\_');
    })
    .join('');
}
