/**
 * Utility functions for calculating and displaying "days in column" badge
 * for task cards. Shows how long a task has been in its current status.
 */

/**
 * Calculate the number of days since the activity date.
 * Returns 0 if activityAt is null or within the last 24 hours.
 */
export function getDaysInColumn(
  activityAt: Date | string | null | undefined
): number {
  if (!activityAt) return 0;

  const activityDate =
    typeof activityAt === 'string' ? new Date(activityAt) : activityAt;

  // Handle invalid dates
  if (isNaN(activityDate.getTime())) return 0;

  const now = new Date();
  const diffMs = now.getTime() - activityDate.getTime();
  const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

  return Math.max(0, diffDays);
}

/**
 * Format days into a display string.
 * Returns null if days is 0 (< 1 day old).
 * Returns "7d+" for 7 or more days.
 */
export function formatDaysInColumn(days: number): string | null {
  if (days < 1) return null;
  if (days >= 7) return '7d+';
  return `${days}d`;
}

/**
 * Get Tailwind classes for styling the days badge based on age.
 * - 1-2 days: neutral/subtle styling
 * - 3-6 days: warning (amber) styling
 * - 7+ days: strong warning (red) styling
 */
export function getDaysStyle(days: number): string {
  if (days < 1) return '';
  if (days <= 2) {
    return 'bg-muted text-muted-foreground';
  }
  if (days <= 6) {
    return 'bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400';
  }
  // 7+ days
  return 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400';
}
