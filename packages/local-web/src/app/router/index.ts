import { createRouter } from '@tanstack/react-router';
import { routeTree } from '@web/routeTree.gen';
import {
  RouterPendingComponent,
  RouterErrorComponent,
} from '@web/app/router/RouterDefaults';

export const router = createRouter({
  routeTree,
  defaultPendingComponent: RouterPendingComponent,
  defaultErrorComponent: RouterErrorComponent,
  defaultPendingMs: 300,
  defaultPendingMinMs: 300,
});

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router;
  }
}
