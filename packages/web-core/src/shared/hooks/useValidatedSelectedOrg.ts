import { useAuth } from '@/shared/hooks/auth/useAuth';
import { useUserOrganizations } from '@/shared/hooks/useUserOrganizations';
import { useOrganizationStore } from '@/shared/stores/useOrganizationStore';

type ValidatedOrgState =
  | { status: 'pending' }
  | { status: 'no-orgs' }
  | { status: 'ready'; orgId: string };

/**
 * Returns a validated organization ID that is safe to mount Electric shapes against.
 *
 * This hook prevents the race condition where ProjectProvider mounts with a stale
 * cached org ID before SharedAppLayout's correction effect runs. Mounting against
 * an invalid org ID produces 403s from Electric, which trigger fallback locking.
 *
 * **IMPORTANT**: This hook depends on SharedAppLayout being mounted above the caller
 * to resolve 'pending' when selectedOrgId is stale. SharedAppLayout's effect at
 * lines 110-121 corrects the org ID when it detects a stale cache.
 */
export function useValidatedSelectedOrg(): ValidatedOrgState {
  const { isLoaded: authLoaded, isSignedIn } = useAuth();
  const { data: orgsData, isLoading: orgsLoading } = useUserOrganizations();
  const selectedOrgId = useOrganizationStore((s) => s.selectedOrgId);

  if (!authLoaded || !isSignedIn) return { status: 'pending' };
  if (orgsLoading || !orgsData) return { status: 'pending' };

  const orgs = orgsData.organizations ?? [];
  if (orgs.length === 0) return { status: 'no-orgs' };

  // "no orgs yet" vs "validation pending":
  //   pending = REST query still in flight
  //   no-orgs = REST resolved with empty list
  //   ready   = REST resolved AND cached id is in the list
  const validCached = selectedOrgId
    ? orgs.some((o) => o.id === selectedOrgId)
    : false;

  if (validCached) {
    // SharedAppLayout's correction effect will leave this id alone — safe to mount.
    return { status: 'ready', orgId: selectedOrgId! };
  }

  // Cached id is stale/missing. SharedAppLayout's effect will correct it on next
  // commit; until then we stay 'pending' to avoid the 403-then-fallback race.
  return { status: 'pending' };
}
