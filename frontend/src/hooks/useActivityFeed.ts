import { useQuery } from '@tanstack/react-query';
import { dashboardApi } from '@/lib/api';
import type { ActivityFeed, ActivityCategory } from 'shared/types';

interface UseActivityFeedOptions {
  /** Filter items by category */
  category?: ActivityCategory;
  /** Include dismissed items in the feed */
  includeDismissed?: boolean;
  /** Enable/disable the query */
  enabled?: boolean;
}

export function useActivityFeed(options?: UseActivityFeedOptions) {
  const { category, includeDismissed = false, enabled = true } = options ?? {};

  const query = useQuery<ActivityFeed>({
    queryKey: ['dashboard', 'activity', { includeDismissed }],
    queryFn: () => dashboardApi.getActivityFeed(includeDismissed),
    refetchInterval: 10000, // Poll every 10 seconds
    staleTime: 5000,
    enabled,
  });

  // Filter items by category if specified
  const filteredItems = category
    ? query.data?.items.filter((item) => item.category === category)
    : query.data?.items;

  return {
    ...query,
    data: query.data
      ? {
          ...query.data,
          items: filteredItems ?? [],
        }
      : undefined,
  };
}
