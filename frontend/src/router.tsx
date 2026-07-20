/* Router construction intentionally co-locates lazy component references with
 * the exported router; it is application bootstrap state, not an HMR leaf. */
/* eslint-disable react-refresh/only-export-components */
import { lazy, Suspense } from "react"
import {
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
} from "@tanstack/react-router"

import { AppShell } from "@/components/layout/app-shell"

const AuditPage = lazy(() => import("@/pages/audit").then((m) => ({ default: m.AuditPage })))
const DashboardPage = lazy(() =>
  import("@/pages/dashboard").then((m) => ({ default: m.DashboardPage })),
)
const DeploymentDetailPage = lazy(() =>
  import("@/pages/deployment-detail").then((m) => ({ default: m.DeploymentDetailPage })),
)
const DeploymentsPage = lazy(() =>
  import("@/pages/deployments").then((m) => ({ default: m.DeploymentsPage })),
)
const HelpGitlabOauthPage = lazy(() =>
  import("@/pages/help-gitlab-oauth").then((m) => ({ default: m.HelpGitlabOauthPage })),
)
const LoginPage = lazy(() => import("@/pages/login").then((m) => ({ default: m.LoginPage })))
const ServerDetailPage = lazy(() =>
  import("@/pages/server-detail").then((m) => ({ default: m.ServerDetailPage })),
)
const ServersPage = lazy(() =>
  import("@/pages/servers").then((m) => ({ default: m.ServersPage })),
)
const SettingsPage = lazy(() =>
  import("@/pages/settings").then((m) => ({ default: m.SettingsPage })),
)
const TelemetryPage = lazy(() =>
  import("@/pages/telemetry").then((m) => ({ default: m.TelemetryPage })),
)

function RootRoute() {
  return (
    <Suspense
      fallback={
        <div className="grid min-h-screen place-items-center" role="status" aria-live="polite">
          <span className="text-sm text-muted-foreground">Loading Foundry…</span>
        </div>
      }
    >
      <Outlet />
    </Suspense>
  )
}

const rootRoute = createRootRoute({ component: RootRoute })

const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/login",
  component: LoginPage,
  validateSearch: (search): { error?: string } =>
    typeof search.error === "string" ? { error: search.error } : {},
})

const appLayout = createRoute({
  getParentRoute: () => rootRoute,
  id: "app",
  component: AppShell,
})

const routeTree = rootRoute.addChildren([
  loginRoute,
  appLayout.addChildren([
    createRoute({ getParentRoute: () => appLayout, path: "/", component: DashboardPage }),
    createRoute({
      getParentRoute: () => appLayout,
      path: "/deployments",
      component: DeploymentsPage,
    }),
    createRoute({
      getParentRoute: () => appLayout,
      path: "/deployments/$deploymentId",
      component: DeploymentDetailPage,
    }),
    createRoute({ getParentRoute: () => appLayout, path: "/servers", component: ServersPage }),
    createRoute({
      getParentRoute: () => appLayout,
      path: "/servers/$serverId",
      component: ServerDetailPage,
    }),
    createRoute({ getParentRoute: () => appLayout, path: "/telemetry", component: TelemetryPage }),
    createRoute({ getParentRoute: () => appLayout, path: "/audit", component: AuditPage }),
    createRoute({ getParentRoute: () => appLayout, path: "/settings", component: SettingsPage }),
    createRoute({
      getParentRoute: () => appLayout,
      path: "/help/gitlab-oauth",
      component: HelpGitlabOauthPage,
    }),
  ]),
])

export const router = createRouter({ routeTree })

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router
  }
}
