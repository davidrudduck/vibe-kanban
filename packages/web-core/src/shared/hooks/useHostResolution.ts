import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { listRelayHosts } from '@/shared/lib/remoteApi';
import { useUserSystem } from '@/shared/hooks/useUserSystem';

const RELAY_HOSTS_QUERY_KEY = ['relay-hosts'] as const;

/**
 * Resolves relay host names from host IDs.
 * Also identifies the local machine's host so local sessions can be self-labeled.
 */
export function useHostResolution() {
  const { machineId } = useUserSystem();

  const { data: relayHosts = [] } = useQuery({
    queryKey: RELAY_HOSTS_QUERY_KEY,
    queryFn: listRelayHosts,
    staleTime: 60_000,
  });

  const hostsById = useMemo(
    () => new Map(relayHosts.map((h) => [h.id, h.name])),
    [relayHosts]
  );

  const localHostEntry = useMemo(
    () =>
      machineId
        ? relayHosts.find((h) => h.machine_id === machineId)
        : undefined,
    [relayHosts, machineId]
  );

  const hasMultipleHosts = relayHosts.length > 1;

  /**
   * Resolve a host_id to a display name.
   * - If host_id is set: look up in hostsById.
   * - If host_id is null/undefined: use local machine's host name (self-label).
   * Returns undefined if there are no relay hosts configured.
   */
  const resolveHostName = useMemo(
    () =>
      (hostId: string | null | undefined): string | undefined => {
        if (hostId) return hostsById.get(hostId);
        // Self-label: null host_id means created locally on this machine
        return localHostEntry?.name;
      },
    [hostsById, localHostEntry]
  );

  return {
    hostsById,
    localHostEntry,
    hasMultipleHosts,
    resolveHostName,
    relayHosts,
  };
}
