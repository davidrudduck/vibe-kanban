import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Label } from '@/components/ui/label';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Loader2, Network } from 'lucide-react';
import { useUserOrganizations } from '@/hooks/useUserOrganizations';
import { useOrganizationSelection } from '@/hooks/useOrganizationSelection';
import { useAuth } from '@/hooks/auth/useAuth';
import { LoginRequiredPrompt } from '@/components/dialogs/shared/LoginRequiredPrompt';
import {
  SwarmProjectsSection,
  NodeProjectsSection,
  SwarmLabelsSection,
  SwarmTemplatesSection,
  NodeTemplatesSection,
} from '@/components/swarm';

export function SwarmSettings() {
  const { t } = useTranslation(['settings', 'common']);
  const { isSignedIn, isLoaded } = useAuth();
  const [error] = useState<string | null>(null);

  // Fetch all organizations
  const {
    data: orgsResponse,
    isLoading: orgsLoading,
    error: orgsError,
  } = useUserOrganizations();

  // Organization selection with URL sync
  const { selectedOrgId, selectedOrg, handleOrgSelect } =
    useOrganizationSelection({
      organizations: orgsResponse,
    });

  if (!isLoaded || orgsLoading) {
    return (
      <div className="flex items-center justify-center py-8">
        <Loader2 className="h-8 w-8 animate-spin" />
        <span className="ml-2">
          {t('settings.swarm.loading', 'Loading swarm settings...')}
        </span>
      </div>
    );
  }

  if (!isSignedIn) {
    return (
      <div className="py-8">
        <LoginRequiredPrompt
          title={t(
            'settings.swarm.loginRequired.title',
            'Sign in to manage swarm'
          )}
          description={t(
            'settings.swarm.loginRequired.description',
            'Sign in to your account to manage swarm projects, labels, and templates across your organizations.'
          )}
          actionLabel={t('settings.swarm.loginRequired.action', 'Sign In')}
        />
      </div>
    );
  }

  if (orgsError) {
    return (
      <div className="py-8">
        <Alert variant="destructive">
          <AlertDescription>
            {orgsError instanceof Error
              ? orgsError.message
              : t('settings.swarm.loadError', 'Failed to load organizations')}
          </AlertDescription>
        </Alert>
      </div>
    );
  }

  const organizations = orgsResponse?.organizations ?? [];

  return (
    <div className="space-y-6">
      {error && (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      {/* Organization Selector */}
      <Card>
        <CardHeader>
          <div className="flex items-center gap-2">
            <Network className="h-5 w-5 text-muted-foreground" />
            <div>
              <CardTitle>
                {t('settings.swarm.title', 'Swarm Management')}
              </CardTitle>
              <CardDescription>
                {t(
                  'settings.swarm.description',
                  'Manage shared projects, labels, and templates across your swarm nodes.'
                )}
              </CardDescription>
            </div>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="swarm-org-selector">
              {t('settings.swarm.selectOrg', 'Select Organization')}
            </Label>
            <Select value={selectedOrgId} onValueChange={handleOrgSelect}>
              <SelectTrigger id="swarm-org-selector" className="max-w-md">
                <SelectValue
                  placeholder={t(
                    'settings.swarm.selectOrgPlaceholder',
                    'Choose an organization...'
                  )}
                />
              </SelectTrigger>
              <SelectContent>
                {organizations.length > 0 ? (
                  organizations.map((org) => (
                    <SelectItem key={org.id} value={org.id}>
                      {org.name}
                    </SelectItem>
                  ))
                ) : (
                  <SelectItem value="no-orgs" disabled>
                    {t('settings.swarm.noOrgs', 'No organizations available')}
                  </SelectItem>
                )}
              </SelectContent>
            </Select>
            <p className="text-sm text-muted-foreground">
              {t(
                'settings.swarm.selectOrgHelper',
                'Select the organization whose swarm settings you want to manage.'
              )}
            </p>
          </div>
        </CardContent>
      </Card>

      {/* Swarm Projects Section */}
      {selectedOrg && <SwarmProjectsSection organizationId={selectedOrg.id} />}

      {/* Node Projects Section - Link local projects to swarm */}
      {selectedOrg && <NodeProjectsSection organizationId={selectedOrg.id} />}

      {/* Swarm Labels Section */}
      {selectedOrg && <SwarmLabelsSection organizationId={selectedOrg.id} />}

      {/* Swarm Templates Section */}
      {selectedOrg && <SwarmTemplatesSection organizationId={selectedOrg.id} />}

      {/* Node Templates Section - Promote local templates to swarm */}
      {selectedOrg && <NodeTemplatesSection organizationId={selectedOrg.id} />}
    </div>
  );
}
