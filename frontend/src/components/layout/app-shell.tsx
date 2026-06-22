import { Link, Navigate, Outlet } from "@tanstack/react-router"
import {
  ActivityIcon,
  CircleHelpIcon,
  LayoutDashboardIcon,
  MenuIcon,
  RocketIcon,
  ScrollTextIcon,
  ServerIcon,
  SettingsIcon,
} from "lucide-react"

import { ModeToggle } from "@/components/mode-toggle"
import { RegistryWatchProvider } from "@/components/registry-watch"
import { UserMenu } from "@/components/user-menu"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Skeleton } from "@/components/ui/skeleton"
import { useMe } from "@/hooks/use-auth"

const NAV = [
  { to: "/", label: "Dashboard", icon: LayoutDashboardIcon },
  { to: "/deployments", label: "Deployments", icon: RocketIcon },
  { to: "/servers", label: "Servers", icon: ServerIcon },
  { to: "/telemetry", label: "Telemetry", icon: ActivityIcon },
  { to: "/audit", label: "Audit Logs", icon: ScrollTextIcon },
  { to: "/settings", label: "Settings", icon: SettingsIcon },
] as const

/** Authenticated layout: top navigation (docs/UI-DESIGN.md § Pages)
 * around every app page; unauthenticated visitors land on /login. */
export function AppShell() {
  const me = useMe()

  if (me.isPending) {
    return (
      <div className="flex min-h-svh items-center justify-center">
        <Skeleton className="h-8 w-40" />
      </div>
    )
  }
  if (!me.data) {
    return <Navigate to="/login" />
  }

  return (
    <RegistryWatchProvider>
      {/* overflow-x-clip: a wide child must never drag the whole page
          sideways on a phone — content scrolls inside its own box. */}
      <div className="flex min-h-svh flex-col overflow-x-clip">
      <header className="sticky top-0 z-10 border-b bg-background/95 backdrop-blur">
        <div className="flex h-14 items-center gap-3 px-4 lg:gap-6">
          {/* Below lg the inline nav would push the header wider than a
              phone viewport (dragging the page with it), so it collapses
              into this menu. Same items, same active styling. */}
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="lg:hidden"
                aria-label="Open navigation menu"
              >
                <MenuIcon className="size-5" aria-hidden />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-52">
              {NAV.map(({ to, label, icon: Icon }) => (
                <DropdownMenuItem key={to} asChild>
                  <Link
                    to={to}
                    activeOptions={{ exact: to === "/" }}
                    activeProps={{ className: "bg-accent text-accent-foreground" }}
                  >
                    <Icon className="size-4" aria-hidden />
                    {label}
                  </Link>
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>

          <Link to="/" className="flex items-center gap-2 font-semibold">
            <span className="flex size-7 items-center justify-center rounded-md bg-primary text-xs font-bold text-primary-foreground">
              F
            </span>
            Foundry
          </Link>
          <nav className="hidden items-center gap-1 text-sm lg:flex" aria-label="Main">
            {NAV.map(({ to, label, icon: Icon }) => (
              <Link
                key={to}
                to={to}
                className="flex items-center gap-1.5 rounded-md px-3 py-1.5 text-muted-foreground transition-colors hover:text-foreground"
                activeOptions={{ exact: to === "/" }}
                activeProps={{ className: "bg-accent text-accent-foreground" }}
              >
                <Icon className="size-4" aria-hidden />
                {label}
              </Link>
            ))}
          </nav>
          <div className="ml-auto flex items-center gap-1">
            <Button variant="ghost" size="icon" asChild>
              <Link to="/help/gitlab-oauth" aria-label="Help">
                <CircleHelpIcon className="size-4" aria-hidden />
              </Link>
            </Button>
            <ModeToggle />
            <UserMenu me={me.data} />
          </div>
        </div>
      </header>
      <main className="flex-1 p-4">
        <Outlet />
      </main>
      </div>
    </RegistryWatchProvider>
  )
}
