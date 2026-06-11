import { useDroppable } from "@dnd-kit/core"
import { Link } from "@tanstack/react-router"
import { ServerOffIcon } from "lucide-react"

import { EmptyState } from "@/components/empty-state"
import { Skeleton } from "@/components/ui/skeleton"
import { useServers } from "@/hooks/use-servers"
import { SERVER_STATUS_META, SLOT_STATE_META } from "@/lib/states"
import type { DropSlotData, GpuSummary, ServerSummary, SlotSummary } from "@/lib/types"
import { cn } from "@/lib/utils"

/** "Servers & GPU Slots" — the dashboard grid (docs/UI-DESIGN.md § 2).
 * Slot chips become drop targets in the deployment phase. */
export function ServerGrid() {
  const servers = useServers()

  if (servers.isPending) {
    return (
      <div className="space-y-3">
        <Skeleton className="h-24 w-full" />
        <Skeleton className="h-24 w-full" />
      </div>
    )
  }
  if (servers.isError) {
    return <EmptyState icon={ServerOffIcon} title="Could not load servers" />
  }
  if (servers.data.length === 0) {
    return (
      <EmptyState
        icon={ServerOffIcon}
        title="No servers enrolled"
        description="Enroll a GPU server with an enrollment token to see its GPUs and MIG slots here."
      />
    )
  }

  return (
    <div className="flex flex-col gap-3">
      {servers.data.map((server) => (
        <ServerRow key={server.id} server={server} />
      ))}
    </div>
  )
}

function ServerRow({ server }: { server: ServerSummary }) {
  const meta = SERVER_STATUS_META[server.status]
  const offline = server.status === "OFFLINE"

  return (
    <div className={cn("rounded-lg border p-3", offline && "opacity-60")}>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <span className={cn("size-2.5 shrink-0 rounded-full", meta.dotClass)} aria-hidden />
        <Link
          to="/servers/$serverId"
          params={{ serverId: server.id }}
          className="font-medium underline-offset-4 hover:underline"
        >
          {server.name}
        </Link>
        {server.hostname ? (
          <span className="text-sm text-muted-foreground">{server.hostname}</span>
        ) : null}
        {server.os_version ? (
          <span className="text-xs text-muted-foreground">{server.os_version}</span>
        ) : null}
        <span className="ml-auto text-xs text-muted-foreground">
          {server.gpus.length} GPU{server.gpus.length === 1 ? "" : "s"}
        </span>
      </div>

      {server.gpus.length === 0 ? (
        <p className="mt-2 text-sm text-muted-foreground">
          {server.enrolled
            ? "No GPUs reported yet — first inventory lands within a minute of the agent starting."
            : "Awaiting enrollment."}
        </p>
      ) : (
        <div className="mt-2 flex flex-col gap-2">
          {server.gpus.map((gpu) => (
            <GpuStrip key={gpu.id} gpu={gpu} server={server} />
          ))}
        </div>
      )}
    </div>
  )
}

function GpuStrip({ gpu, server }: { gpu: GpuSummary; server: ServerSummary }) {
  return (
    <div>
      <p className="mb-1 text-xs font-medium text-muted-foreground">
        GPU {gpu.index} {gpu.model ? `(${gpu.model})` : ""}
        {gpu.mig_enabled ? (
          <span className="ml-2 text-slot-free">MIG: Enabled</span>
        ) : (
          <span className="ml-2">No MIG</span>
        )}
      </p>
      <div className="flex flex-wrap gap-1.5">
        {gpu.slots.map((slot) => (
          <SlotChip key={slot.id} slot={slot} server={server} />
        ))}
      </div>
    </div>
  )
}

/** Drop target: FREE → deploy; RUNNING → replace (with confirmation in
 * the dialog). Other states don't activate (docs/UI-DESIGN.md). */
function SlotChip({ slot, server }: { slot: SlotSummary; server: ServerSummary }) {
  const meta = SLOT_STATE_META[slot.state]
  const droppable = slot.state === "FREE" || slot.state === "RUNNING"
  const data: DropSlotData = {
    slotId: slot.id,
    slotName: slot.name,
    slotState: slot.state,
    serverId: server.id,
    serverName: server.name,
  }
  const { setNodeRef, isOver } = useDroppable({
    id: slot.id,
    data,
    disabled: !droppable || server.status !== "ONLINE",
  })
  const capacity =
    slot.mig_profile ??
    (slot.capacity_mb != null ? `${Math.round(slot.capacity_mb / 1024)} GB` : "")

  return (
    <div
      ref={setNodeRef}
      className={cn(
        "flex min-w-20 flex-col items-center rounded-md border px-3 py-1.5 transition-colors",
        slot.state === "FREE" && "border-slot-free/50",
        slot.state === "RUNNING" && "border-slot-running/60 bg-slot-running/10",
        slot.state === "OFFLINE" && "border-dashed opacity-60",
        (slot.state === "RESERVED" || slot.state === "DEPLOYING" || slot.state === "STOPPING") &&
          "border-slot-reserved/60 bg-slot-reserved/10",
        slot.state === "FAILED" && "border-slot-failed/60 bg-slot-failed/10",
        isOver && slot.state === "FREE" && "border-2 border-dashed border-slot-free bg-slot-free/15",
        isOver &&
          slot.state === "RUNNING" &&
          "border-2 border-dashed border-slot-reserved bg-slot-reserved/15",
      )}
      title={`${meta.label} · ${slot.slot_type}${slot.state === "RUNNING" ? " — drop to replace" : ""}`}
    >
      <span className="font-mono text-xs font-medium">{slot.name}</span>
      <span className={cn("text-[10px]", meta.textClass)}>{capacity || meta.label}</span>
    </div>
  )
}
