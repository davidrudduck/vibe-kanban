import { useEffect } from 'react';
import { WarningIcon, ArrowClockwiseIcon } from '@phosphor-icons/react';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import { getFirstProjectDestination } from '@/shared/lib/firstProjectDestination';
import { useOrganizationStore } from '@/shared/stores/useOrganizationStore';
import { useUiPreferencesStore } from '@/shared/stores/useUiPreferencesStore';
import { useAppNavigation } from '@/shared/hooks/useAppNavigation';

export function RootRedirectPage() {
  const { config, loading, loginStatus } = useUserSystem();
  const setSelectedOrgId = useOrganizationStore((s) => s.setSelectedOrgId);
  const appNavigation = useAppNavigation();

  useEffect(() => {
    if (loading || !config) {
      return;
    }

    let isActive = true;
    void (async () => {
      if (!config.remote_onboarding_acknowledged) {
        appNavigation.goToOnboarding({ replace: true });
        return;
      }

      if (loginStatus?.status !== 'loggedin') {
        appNavigation.goToWorkspacesCreate({ replace: true });
        return;
      }

      // Read saved selections imperatively to avoid re-triggering this effect
      // when the scratch store initializes from the server
      const { selectedOrgId, selectedProjectId } =
        useUiPreferencesStore.getState();

      const destination = await getFirstProjectDestination(
        setSelectedOrgId,
        selectedOrgId,
        selectedProjectId
      );
      if (!isActive) {
        return;
      }

      if (destination?.kind === 'project') {
        appNavigation.goToProject(destination.projectId, { replace: true });
        return;
      }

      appNavigation.goToWorkspacesCreate({ replace: true });
    })();

    return () => {
      isActive = false;
    };
  }, [appNavigation, config, loading, loginStatus?.status, setSelectedOrgId]);

  // Backend unavailable — loading finished but no config arrived
  if (!loading && !config) {
    return (
      <div className="h-screen bg-primary flex flex-col items-center justify-center gap-double p-double">
        <WarningIcon className="size-12 text-error" weight="fill" />
        <div className="flex flex-col items-center gap-half text-center">
          <p className="text-base font-semibold text-high">Unable to connect</p>
          <p className="text-sm text-low max-w-xs">
            The Vibe Kanban backend is not responding. Make sure the app is
            running and try again.
          </p>
        </div>
        <button
          type="button"
          onClick={() => window.location.reload()}
          className="flex items-center gap-half rounded-md bg-brand px-double py-base text-sm font-medium text-white hover:bg-brand/90 transition-colors"
        >
          <ArrowClockwiseIcon className="size-icon-base" weight="bold" />
          Retry
        </button>
      </div>
    );
  }

  return (
    <div className="h-screen bg-primary flex items-center justify-center">
      <div className="size-6 animate-spin rounded-full border-2 border-muted border-t-brand" />
    </div>
  );
}
