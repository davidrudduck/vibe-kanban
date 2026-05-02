/**
 * Validates that a URL uses an allowed scheme (http or https).
 * Returns the URL if safe, or undefined if it is invalid or uses a
 * disallowed scheme (e.g. javascript:, data:, vbscript:).
 */
export function safeUrl(url: string | null | undefined): string | undefined {
  if (!url || !url.trim()) return undefined;
  try {
    const { protocol } = new URL(url.trim());
    return ['http:', 'https:'].includes(protocol) ? url.trim() : undefined;
  } catch {
    return undefined;
  }
}
