import {
  Outlet,
  createRootRoute,
  createRoute,
  createRouter,
  redirect,
} from "@tanstack/react-router";

import { AuthLayout } from "@/components/layout";
import { setUnauthorizedHandler } from "@/lib/api";
import { getCredentials } from "@/lib/auth";
import { z } from "zod";
import { AuthRoute } from "@/routes/auth";
import { DashboardRoute } from "@/routes/dashboard";
import { FeatureFlagsRoute } from "@/routes/feature-flags";
import { FeedbackRoute } from "@/routes/feedback";
import { LoginRoute } from "@/routes/login";
import { NoticesRoute } from "@/routes/notices";
import { NotificationsRoute } from "@/routes/notifications";
import { SettingsRoute } from "@/routes/settings";
import { SyncRoute } from "@/routes/sync";
import { UserDetailRoute } from "@/routes/user-detail";

const rootRoute = createRootRoute({
  component: () => <Outlet />,
});

const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/login",
  component: LoginRoute,
  beforeLoad: () => {
    if (getCredentials()) {
      // eslint-disable-next-line @typescript-eslint/only-throw-error -- TanStack Router's `redirect` is thrown by design.
      throw redirect({ to: "/" });
    }
  },
});

const layoutRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: "layout",
  component: () => (
    <AuthLayout>
      <Outlet />
    </AuthLayout>
  ),
  beforeLoad: () => {
    if (!getCredentials()) {
      // eslint-disable-next-line @typescript-eslint/only-throw-error -- TanStack Router's `redirect` is thrown by design.
      throw redirect({ to: "/login" });
    }
  },
});

const indexRoute = createRoute({
  getParentRoute: () => layoutRoute,
  path: "/",
  component: DashboardRoute,
});

const settingsRoute = createRoute({
  getParentRoute: () => layoutRoute,
  path: "settings",
  component: SettingsRoute,
});

const authRoute = createRoute({
  getParentRoute: () => layoutRoute,
  path: "auth",
  component: AuthRoute,
});

const userDetailRoute = createRoute({
  getParentRoute: () => layoutRoute,
  path: "auth/users/$id",
  component: UserDetailRoute,
});

const noticesRoute = createRoute({
  getParentRoute: () => layoutRoute,
  path: "notices",
  component: NoticesRoute,
});

const notificationsRoute = createRoute({
  getParentRoute: () => layoutRoute,
  path: "notifications",
  component: NotificationsRoute,
});

const syncRoute = createRoute({
  getParentRoute: () => layoutRoute,
  path: "sync",
  component: SyncRoute,
});

const feedbackRoute = createRoute({
  getParentRoute: () => layoutRoute,
  path: "feedback",
  component: FeedbackRoute,
  validateSearch: z.object({
    handled: z.enum(["all", "new", "archived"]).optional(),
  }),
});

const featureFlagsRoute = createRoute({
  getParentRoute: () => layoutRoute,
  path: "feature-flags",
  component: FeatureFlagsRoute,
  validateSearch: z.object({
    flag: z.string().optional(),
  }),
});

// Legacy bookmarks from the pre-consolidation nav → single Auth page.
function redirectRoute(path: string) {
  return createRoute({
    getParentRoute: () => layoutRoute,
    path,
    beforeLoad: () => {
      // eslint-disable-next-line @typescript-eslint/only-throw-error -- TanStack Router's `redirect` is thrown by design.
      throw redirect({ to: "/auth" });
    },
    component: () => null,
  });
}

const accountsRedirect = redirectRoute("accounts");
const sessionsRedirect = redirectRoute("sessions");
const authProvidersRedirect = redirectRoute("auth-providers");

const routeTree = rootRoute.addChildren([
  loginRoute,
  layoutRoute.addChildren([
    indexRoute,
    settingsRoute,
    authRoute,
    userDetailRoute,
    noticesRoute,
    notificationsRoute,
    syncRoute,
    feedbackRoute,
    featureFlagsRoute,
    accountsRedirect,
    sessionsRedirect,
    authProvidersRedirect,
  ]),
]);

export const router = createRouter({
  routeTree,
  defaultPreload: "intent",
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

export function wireUnauthorizedRedirect() {
  setUnauthorizedHandler(() => {
    void router.navigate({ to: "/login" });
  });
}
