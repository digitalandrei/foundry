import { Link, Outlet } from "@tanstack/react-router"
import { LayoutDashboardIcon, RocketIcon, ServerIcon, ScrollTextIcon, SettingsIcon } from "lucide-react"

import { ModeToggle } from "@/components/mode-toggle"

const NAV = [
  { to: "/", label: "Dashboard", icon: LayoutDashboardIcon },
  { to: "/deployments", label: "Deployments", icon: RocketIcon },
  { to: "/servers", label: "Servers", icon: ServerIcon },
  { to: "/audit", label: "Audit Logs", icon: ScrollTextIcon },
  { to: "/settings", label: "Settings", icon: SettingsIcon },
] as const

/** Top navigation per docs/UI-DESIGN.md § Pages. */
export function AppShell() {
  return (
    <div className="flex min-h-svh flex-col">
      <header className="sticky top-0 z-10 border-b bg-background/95 backdrop-blur">
        <div className="flex h-14 items-center gap-6 px-4">
          <Link to="/" className="flex items-center gap-2 font-semibold">
            <span className="flex size-7 items-center justify-center rounded-md bg-primary text-primary-foreground text-xs font-bold">
              F
            </span>
            Foundry
          </Link>
          <nav className="flex items-center gap-1 text-sm" aria-label="Main">
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
          <div className="ml-auto">
            <ModeToggle />
          </div>
        </div>
      </header>
      <main className="flex-1 p-4">
        <Outlet />
      </main>
    </div>
  )
}
