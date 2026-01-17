/**
 * Stale time for projects query cache (30 seconds).
 * Projects data doesn't change frequently, so we can cache for a reasonable duration.
 */
export const PROJECTS_STALE_TIME_MS = 30_000;

/**
 * Stale time for swarm sync health query cache (5 minutes).
 * Sync health status is checked periodically but doesn't need real-time updates.
 */
export const SYNC_HEALTH_STALE_TIME_MS = 300_000;
