import { useState } from "react"
import { DndContext, DragOverlay, type DragEndEvent, type DragStartEvent } from "@dnd-kit/core"
import { Link } from "@tanstack/react-router"
import { PackageIcon } from "lucide-react"
import { toast } from "sonner"

import { ContainersPanel } from "@/components/containers-panel"
import { DeployDialog, type DeployTarget } from "@/components/deploy-dialog"
import { ServerGrid } from "@/components/server-grid"
import { SlotLegend } from "@/components/slot-legend"
import { useDeployments } from "@/hooks/use-deployments"
import { useServers } from "@/hooks/use-servers"
import { formatRelative, formatSize } from "@/lib/format"
import { DEPLOYMENT_STATE_META } from "@/lib/states"
import type { DragTagData, DropSlotData } from "@/lib/types"
import { Badge } from "@/components/ui/badge"
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
 * sidebar, slot grid (drop targets), running-deployments panel. */
export function DashboardPage() {
  const deployments = useDeployments()
  const [dragging, setDragging] = useState<DragTagData | null>(null)
  const [target, setTarget] = useState<DeployTarget | null>(null)

  const onDragStart = (e: DragStartEvent) => {
    setDragging((e.active.data.current as DragTagData) ?? null)
  }
  const onDragEnd = (e: DragEndEvent) => {
    setDragging(null)
    const tag = e.active.data.current as DragTagData | undefined
    const slot = e.over?.data.current as DropSlotData | undefined
    if (!tag || !slot) return
    if (slot.slotState === "FREE") {
      setTarget({ tag, slot, replaces: null })
      return
    }
    // Occupied slot → replacement: confirm against what's running.
    const current = (deployments.data ?? []).find(
      (d) => d.slot_id === slot.slotId && d.state === "RUNNING",
    )
    if (!current) {
      toast.error("Slot is busy with an operation in progress — try again shortly.")
      return
    }
    setTarget({ tag, slot, replaces: current })
  }

  // The dashboard table shows everything still alive on the fleet —
  // including in-flight deploys (live progress) and failures. History
  // (REPLACED) stays on the Deployments page.
  const active = (deployments.data ?? []).filter((d) => d.state !== "REPLACED")

  return (
    // App-like layout: on wide screens the page never scrolls — every
    // box scrolls inside itself (header h-14 + main p-4 → 5.5rem of
    // chrome). Below lg the boxes stack and the page scrolls normally.
    <DndContext onDragStart={onDragStart} onDragEnd={onDragEnd}>
    <div className="flex h-auto flex-col gap-4 lg:h-[calc(100svh-5.5rem)] lg:flex-row">
      <aside className="flex w-full shrink-0 flex-col gap-4 lg:w-72">
        <Card className="flex max-h-[60svh] min-h-0 flex-1 flex-col lg:max-h-none">
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

      <section className="flex min-w-0 flex-1 flex-col gap-4">
        <Card className="flex min-h-0 flex-col lg:flex-[3]">
          <CardHeader className="shrink-0 flex-row items-center justify-between">
            <CardTitle className="text-base">Servers &amp; GPU Slots (NVIDIA MIG)</CardTitle>
            <SlotLegend />
          </CardHeader>
          <CardContent className="min-h-0 flex-1 lg:overflow-y-auto">
            <ServerGrid />
          </CardContent>
        </Card>

        <Card className="flex min-h-0 flex-col lg:flex-[2]">
          <CardHeader className="shrink-0 flex-row items-center justify-between">
            <CardTitle className="text-base">
              Deployments
              {deployments.data ? (
                <Badge variant="secondary" className="ml-2">
                  {deployments.data.filter((d) => d.state === "RUNNING").length} running
                </Badge>
              ) : null}
            </CardTitle>
            <Link to="/deployments" className="text-sm text-muted-foreground hover:text-foreground">
              View All
            </Link>
          </CardHeader>
          <CardContent className="min-h-0 flex-1 lg:overflow-y-auto">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Image</TableHead>
                  <TableHead>Server</TableHead>
                  <TableHead>GPU / Slice</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Uptime</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {active.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={6} className="h-20 text-center text-muted-foreground">
                      No deployments yet — drag an image onto a free slot.
                    </TableCell>
                  </TableRow>
                ) : (
                  active.map((d) => (
                    <TableRow key={d.id}>
                      <TableCell className="font-medium">{d.name}</TableCell>
                      <TableCell
                        className="max-w-56 truncate font-mono text-xs text-muted-foreground"
                        title={d.image_ref}
                      >
                        {d.image_ref}
                      </TableCell>
                      <TableCell>{d.server_name}</TableCell>
                      <TableCell className="font-mono text-xs">
                        {d.gpu_label} / {d.slot_name}
                      </TableCell>
                      <TableCell>
                        <span className={DEPLOYMENT_STATE_META[d.state].textClass}>
                          {DEPLOYMENT_STATE_META[d.state].label}
                        </span>
                        {d.status_detail ? (
                          <span className="block max-w-52 truncate font-mono text-[10px] text-muted-foreground">
                            {d.status_detail}
                          </span>
                        ) : null}
                        {d.state === "FAILED" && d.error_message ? (
                          <span
                            className="block max-w-52 truncate text-[10px] text-muted-foreground"
                            title={d.error_message}
                          >
                            {d.error_message}
                          </span>
                        ) : null}
                      </TableCell>
                      <TableCell className="text-muted-foreground">
                        {d.state === "RUNNING" && d.started_at
                          ? formatRelative(d.started_at).replace(" ago", "")
                          : "—"}
                      </TableCell>
                    </TableRow>
                  ))
                )}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      </section>
    </div>

    <DragOverlay>
      {dragging ? (
        <div className="flex items-center gap-2 rounded-md border bg-card px-3 py-2 text-sm shadow-lg">
          <PackageIcon className="size-4 text-muted-foreground" aria-hidden />
          <span>{dragging.imageName}</span>
          <Badge variant="secondary" className="font-mono text-[10px]">
            {dragging.tagName}
          </Badge>
          {dragging.sizeBytes != null ? (
            <span className="text-xs text-muted-foreground">{formatSize(dragging.sizeBytes)}</span>
          ) : null}
        </div>
      ) : null}
    </DragOverlay>

    <DeployDialog target={target} onClose={() => setTarget(null)} />
    </DndContext>
  )
}
