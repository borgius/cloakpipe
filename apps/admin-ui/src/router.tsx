import {
  createRootRoute,
  createRoute,
  createRouter,
} from '@tanstack/react-router';
import { RootLayout } from './components/RootLayout';
import { OverviewPage } from './routes/Overview';
import { ProfilesPage } from './routes/Profiles';
import { PoliciesPage } from './routes/Policies';
import { CategoriesPage } from './routes/Categories';
import { AuditPage } from './routes/Audit';
import { VaultPage } from './routes/Vault';
import { SessionsPage } from './routes/Sessions';

const rootRoute = createRootRoute({ component: RootLayout });

const routes = [
  createRoute({ getParentRoute: () => rootRoute, path: '/', component: OverviewPage }),
  createRoute({ getParentRoute: () => rootRoute, path: '/profiles', component: ProfilesPage }),
  createRoute({ getParentRoute: () => rootRoute, path: '/policies', component: PoliciesPage }),
  createRoute({ getParentRoute: () => rootRoute, path: '/categories', component: CategoriesPage }),
  createRoute({ getParentRoute: () => rootRoute, path: '/audit', component: AuditPage }),
  createRoute({ getParentRoute: () => rootRoute, path: '/vault', component: VaultPage }),
  createRoute({ getParentRoute: () => rootRoute, path: '/sessions', component: SessionsPage }),
];

const routeTree = rootRoute.addChildren(routes);

export const router = createRouter({ routeTree });

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router;
  }
}
