import { useQuery } from '@tanstack/react-query';
import { dashboardApi } from '@/lib/api';
import type { DashboardSummary } from 'shared/types';

export function useDashboardSummary() {
  return useQuery<DashboardSummary>({
    queryKey: ['dashboard', 'summary'],
    queryFn: () => dashboardApi.getSummary(),
    // Only poll when tab is visible to reduce unnecessary network requests
    refetchInterval: () => (document.hidden ? false : 5000),
    staleTime: 2000,
  });
}
