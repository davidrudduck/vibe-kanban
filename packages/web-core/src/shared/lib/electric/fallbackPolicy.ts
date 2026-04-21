export function shouldLockSourceToFallbackForStatus(
  status: number | undefined
): boolean {
  return (
    status === 403 || status === 404 || status === undefined || status >= 500
  );
}
