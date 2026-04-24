import { RouterProvider } from "@tanstack/react-router";
import { HotkeysProvider } from "react-hotkeys-hook";
import { router } from "@remote/app/router";
import { AppRuntimeProvider } from "@/shared/hooks/useAppRuntime";
import { ToastViewport } from "@vibe/ui/components/Toast";

export function AppRouter() {
  return (
    <AppRuntimeProvider runtime="remote">
      <HotkeysProvider
        initiallyActiveScopes={["global", "workspace", "kanban", "projects"]}
      >
        <ToastViewport>
          <RouterProvider router={router} />
        </ToastViewport>
      </HotkeysProvider>
    </AppRuntimeProvider>
  );
}
