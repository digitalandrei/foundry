import { ContainersPanel } from "@/components/containers-panel"
import { ServerGrid } from "@/components/server-grid"
import { SlotLegend } from "@/components/slot-legend"
import { useServers } from "@/hooks/use-servers"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Separator } from "@/components/ui/separator"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { APP_VERSION } from "@/lib/version"

function SystemStatus() {
  const servers = useServers()
  const online = (servers.data ?? []).filter((s) => s.status === "ONLINE").length
  const gpus = (servers.data ?? []).reduce((n, s) => n + s.gpus.length, 0)
  const running = (servers.data ?? []).reduce((n, s) => n + s.containers_running, 0)

  return (
    <>
      <div className="flex justify-between">
        <span className="text-muted-foreground">Servers</span>
        <span className={online > 0 ? "text-slot-free" : undefined}>{online} online</span>
      </div>
      <div className="flex justify-between">
        <span className="text-muted-foreground">GPUs</span>
        <span>{gpus} total</span>
      </div>
      <div className="flex justify-between">
        <span className="text-muted-foreground">Containers</span>
        <span>{running} running</span>
      </div>
    </>
  )
}

/** The core screen (docs/UI-DESIGN.md § Dashboard Layout): GitLab
 * sidebar, slot grid, running-deployments panel. Data wiring arrives
 * with the API phases; the empty states below are the real ones. */
export function DashboardPage() {
  return (
    // App-like layout: the page itself never scrolls — the containers
    // tree and the main column each scroll independently
    // (header h-14 + main p-4 → 5.5rem of chrome).
    <div className="flex h-[calc(100svh-5.5rem)] gap-4">
      <aside className="flex w-72 shrink-0 flex-col gap-4">
        <Card className="flex min-h-0 flex-1 flex-col">
          <CardHeader className="shrink-0">
            <CardTitle className="text-sm">Available Containers (from GitLab)</CardTitle>
          </CardHeader>
          <CardContent className="min-h-0 flex-1">
            <ContainersPanel />
          </CardContent>
        </Card>
        <Card className="shrink-0">
          <CardHeader>
            <CardTitle className="text-sm">System Status</CardTitle>
          </CardHeader>
          <CardContent className="space-y-1 text-sm">
            <SystemStatus />
            <Separator className="my-2" />
            <p className="text-xs text-muted-foreground">Foundry v{APP_VERSION}</p>
          </CardContent>
        </Card>
      </aside>

      <section className="flex min-w-0 flex-1 flex-col gap-4 overflow-y-auto pr-1">
        <Card className="shrink-0">
          <CardHeader className="flex-row items-center justify-between">
            <CardTitle className="text-base">Servers &amp; GPU Slots (NVIDIA MIG)</CardTitle>
            <SlotLegend />
          </CardHeader>
          <CardContent>
            <ServerGrid />
          </CardContent>
        </Card>

        <Card className="shrink-0">
          <CardHeader>
            <CardTitle className="text-base">Running Deployments</CardTitle>
          </CardHeader>
          <CardContent>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Image</TableHead>
                  <TableHead>Version</TableHead>
                  <TableHead>Server</TableHead>
                  <TableHead>GPU / Slice</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Uptime</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                <TableRow>
                  <TableCell colSpan={7} className="h-20 text-center text-muted-foreground">
                    No running deployments.
                  </TableCell>
                </TableRow>
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      </section>
    </div>
  )
}
