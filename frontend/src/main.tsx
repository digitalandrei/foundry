import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import {
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  RouterProvider,
} from "@tanstack/react-router"
import { ThemeProvider } from "next-themes"

import { AppShell } from "@/components/layout/app-shell"
import { Toaster } from "@/components/ui/sonner"
import { TooltipProvider } from "@/components/ui/tooltip"
import { AuditPage } from "@/pages/audit"
import { DashboardPage } from "@/pages/dashboard"
import { DeploymentsPage } from "@/pages/deployments"
import { HelpGitlabOauthPage } from "@/pages/help-gitlab-oauth"
import { LoginPage } from "@/pages/login"
import { ServerDetailPage } from "@/pages/server-detail"
import { ServersPage } from "@/pages/servers"
import { SettingsPage } from "@/pages/settings"

import "./index.css"

const rootRoute = createRootRoute({ component: Outlet })

// /login renders standalone; everything else lives under the
// session-guarded AppShell layout.
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
    createRoute({ getParentRoute: () => appLayout, path: "/servers", component: ServersPage }),
    createRoute({
      getParentRoute: () => appLayout,
      path: "/servers/$serverId",
      component: ServerDetailPage,
    }),
    createRoute({ getParentRoute: () => appLayout, path: "/audit", component: AuditPage }),
    createRoute({ getParentRoute: () => appLayout, path: "/settings", component: SettingsPage }),
    createRoute({
      getParentRoute: () => appLayout,
      path: "/help/gitlab-oauth",
      component: HelpGitlabOauthPage,
    }),
  ]),
])

const router = createRouter({ routeTree })

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router
  }
}

const queryClient = new QueryClient()

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ThemeProvider attribute="class" defaultTheme="dark" storageKey="foundry-theme" enableSystem>
      <QueryClientProvider client={queryClient}>
        {/* One provider for every Radix tooltip in the app (a bare
            <Tooltip> throws without it — docs/FRONTEND_RULES.md). */}
        <TooltipProvider>
          <RouterProvider router={router} />
        </TooltipProvider>
        <Toaster />
      </QueryClientProvider>
    </ThemeProvider>
  </StrictMode>,
)
