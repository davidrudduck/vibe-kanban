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

  /**
   * Resolve a host_id to a display name.
   * - Non-null host_id: always shows — returns the known name, or a short
   *   sentinel ("Unknown (abc123)") when the host has been unpaired/deleted.
   * - Null/undefined host_id: self-labels with the local machine's host name,
   *   but only when >1 relay host is configured (avoids noise for solo users).
   */
  const resolveHostName = useMemo(
    () =>
      (hostId: string | null | undefined): string | undefined => {
        if (hostId) return hostsById.get(hostId) ?? `Unknown (${hostId.slice(0, 6)})`;
        // Self-label only when multiple hosts exist (single-host = obvious)
        if (relayHosts.length > 1) return localHostEntry?.name;
        return undefined;
      },
    [hostsById, localHostEntry, relayHosts.length]
  );

  return {
    hostsById,
    localHostEntry,
    resolveHostName,
    relayHosts,
  };
}
