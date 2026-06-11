import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import {
  createRootRoute,
  createRoute,
  createRouter,
  RouterProvider,
} from "@tanstack/react-router"
import { ThemeProvider } from "next-themes"

import { AppShell } from "@/components/layout/app-shell"
import { Toaster } from "@/components/ui/sonner"
import { AuditPage } from "@/pages/audit"
import { DashboardPage } from "@/pages/dashboard"
import { DeploymentsPage } from "@/pages/deployments"
import { ServersPage } from "@/pages/servers"
import { SettingsPage } from "@/pages/settings"

import "./index.css"

const rootRoute = createRootRoute({ component: AppShell })

const routeTree = rootRoute.addChildren([
  createRoute({ getParentRoute: () => rootRoute, path: "/", component: DashboardPage }),
  createRoute({ getParentRoute: () => rootRoute, path: "/deployments", component: DeploymentsPage }),
  createRoute({ getParentRoute: () => rootRoute, path: "/servers", component: ServersPage }),
  createRoute({ getParentRoute: () => rootRoute, path: "/audit", component: AuditPage }),
  createRoute({ getParentRoute: () => rootRoute, path: "/settings", component: SettingsPage }),
])

const router = createRouter({ routeTree })

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router
  }
}

// Server state lives in TanStack Query (docs/FRONTEND_RULES.md); hooks
// arrive with the first API wiring.
const queryClient = new QueryClient()

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ThemeProvider attribute="class" defaultTheme="dark" storageKey="foundry-theme" enableSystem>
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
        <Toaster />
      </QueryClientProvider>
    </ThemeProvider>
  </StrictMode>,
)
