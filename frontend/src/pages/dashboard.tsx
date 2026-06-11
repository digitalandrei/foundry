import { ServerOffIcon } from "lucide-react"

import { ContainersPanel } from "@/components/containers-panel"
import { EmptyState } from "@/components/empty-state"
import { SlotLegend } from "@/components/slot-legend"
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

/** The core screen (docs/UI-DESIGN.md § Dashboard Layout): GitLab
 * sidebar, slot grid, running-deployments panel. Data wiring arrives
 * with the API phases; the empty states below are the real ones. */
export function DashboardPage() {
  return (
    <div className="flex gap-4">
      <aside className="flex w-72 shrink-0 flex-col gap-4">
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Available Containers (from GitLab)</CardTitle>
          </CardHeader>
          <CardContent>
            <ContainersPanel />
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">System Status</CardTitle>
          </CardHeader>
          <CardContent className="space-y-1 text-sm">
            <div className="flex justify-between">
              <span className="text-muted-foreground">Servers</span>
              <span>0 online</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">GPUs</span>
              <span>0 total</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Containers</span>
              <span>0 running</span>
            </div>
            <Separator className="my-2" />
            <p className="text-xs text-muted-foreground">Foundry v{APP_VERSION}</p>
          </CardContent>
        </Card>
      </aside>

      <section className="flex min-w-0 flex-1 flex-col gap-4">
        <Card>
          <CardHeader className="flex-row items-center justify-between">
            <CardTitle className="text-base">Servers &amp; GPU Slots (NVIDIA MIG)</CardTitle>
            <SlotLegend />
          </CardHeader>
          <CardContent>
            <EmptyState
              icon={ServerOffIcon}
              title="No servers enrolled"
              description="Enroll a GPU server with an enrollment token to see its GPUs and MIG slots here."
            />
          </CardContent>
        </Card>

        <Card>
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
