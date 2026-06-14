import { useState } from "react"
import { DndContext, DragOverlay, type DragEndEvent, type DragStartEvent } from "@dnd-kit/core"
// No drop animation: the card snaps to the slot (opening the dialog)
// instead of flying back to the sidebar on release (operator request).
const NO_DROP_ANIMATION = null
import { PackageIcon } from "lucide-react"
import { toast } from "sonner"

import { ContainersPanel } from "@/components/containers-panel"
import { DeployDialog, type DeployTarget } from "@/components/deploy-dialog"
import { ServerGrid } from "@/components/server-grid"
import { SlotLegend } from "@/components/slot-legend"
import { useDeployments } from "@/hooks/use-deployments"
import { useServers } from "@/hooks/use-servers"
import { formatSize } from "@/lib/format"
import type { DragTagData, DropSlotData } from "@/lib/types"
import { Badge } from "@/components/ui/badge"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Separator } from "@/components/ui/separator"
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
        <Card className="flex min-h-0 flex-1 flex-col">
          <CardHeader className="shrink-0 flex-row items-center justify-between">
            <CardTitle className="text-base">Servers &amp; GPU Slots</CardTitle>
            <SlotLegend />
          </CardHeader>
          <CardContent className="min-h-0 flex-1 lg:overflow-y-auto">
            <ServerGrid />
          </CardContent>
        </Card>
      </section>
    </div>

    <DragOverlay dropAnimation={NO_DROP_ANIMATION}>
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
