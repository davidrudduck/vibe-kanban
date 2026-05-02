import React from 'react';
import { RouterProvider } from '@tanstack/react-router';
import { HotkeysProvider } from 'react-hotkeys-hook';
import { UserSystemProvider } from '@web/app/providers/ConfigProvider';
import { ClickedElementsProvider } from '@web/app/providers/ClickedElementsProvider';
import { localAppNavigation } from '@web/app/navigation/AppNavigation';
import { LocalAuthProvider } from '@/shared/providers/auth/LocalAuthProvider';
import { AppRuntimeProvider } from '@/shared/hooks/useAppRuntime';
import { AppNavigationProvider } from '@/shared/hooks/useAppNavigation';
import { useTauriNotificationNavigation } from '@web/app/hooks/useTauriNotificationNavigation';
import { useTauriUpdateReady } from '@web/app/hooks/useTauriUpdateReady';
import { AppSystemNotifications } from '@web/app/notifications/AppSystemNotifications';
import { router } from '@web/app/router';
import { ToastViewport } from '@vibe/ui/components/Toast';
import { FontProvider } from '@/shared/components/FontProvider';
import { AccentProvider } from '@/shared/components/AccentProvider';
import { useUserSystem } from '@/shared/hooks/useUserSystem';

function TauriListeners() {
  useTauriNotificationNavigation();
  useTauriUpdateReady();
  return null;
}

function FontProviderWithConfig({ children }: { children: React.ReactNode }) {
  const { config } = useUserSystem();
  return (
    <FontProvider initialFonts={config?.appearance?.fonts}>
      {children}
    </FontProvider>
  );
}

function AccentProviderWithConfig({ children }: { children: React.ReactNode }) {
  const { config } = useUserSystem();
  return (
    <AccentProvider initialAccent={config?.appearance?.accent_color}>
      {children}
    </AccentProvider>
  );
}

function App() {
  return (
    <AppRuntimeProvider runtime="local">
      <AppNavigationProvider value={localAppNavigation}>
        <TauriListeners />
        <UserSystemProvider>
          <FontProviderWithConfig>
            <AccentProviderWithConfig>
              <LocalAuthProvider>
                <AppSystemNotifications />
                <ClickedElementsProvider>
                  <HotkeysProvider
                    initiallyActiveScopes={[
                      'global',
                      'workspace',
                      'kanban',
                      'projects',
                    ]}
                  >
                    <ToastViewport>
                      <RouterProvider router={router} />
                    </ToastViewport>
                  </HotkeysProvider>
                </ClickedElementsProvider>
              </LocalAuthProvider>
            </AccentProviderWithConfig>
          </FontProviderWithConfig>
        </UserSystemProvider>
      </AppNavigationProvider>
    </AppRuntimeProvider>
  );
}

export default App;
