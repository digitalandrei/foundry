import { ServerOffIcon } from "lucide-react"

import { EmptyState } from "@/components/empty-state"
import { Skeleton } from "@/components/ui/skeleton"
import { useServers } from "@/hooks/use-servers"
import { SERVER_STATUS_META, SLOT_STATE_META } from "@/lib/states"
import type { GpuSummary, ServerSummary, SlotSummary } from "@/lib/types"
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
        <span className="font-medium">{server.name}</span>
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
          {server.gpus.map((gpu, i) => (
            <GpuStrip key={gpu.id} gpu={gpu} ordinal={i} />
          ))}
        </div>
      )}
    </div>
  )
}

function GpuStrip({ gpu, ordinal }: { gpu: GpuSummary; ordinal: number }) {
  return (
    <div>
      <p className="mb-1 text-xs font-medium text-muted-foreground">
        GPU {ordinal} {gpu.model ? `(${gpu.model})` : ""}
        {gpu.mig_enabled ? (
          <span className="ml-2 text-slot-free">MIG: Enabled</span>
        ) : (
          <span className="ml-2">No MIG</span>
        )}
      </p>
      <div className="flex flex-wrap gap-1.5">
        {gpu.slots.map((slot) => (
          <SlotChip key={slot.id} slot={slot} />
        ))}
      </div>
    </div>
  )
}

function SlotChip({ slot }: { slot: SlotSummary }) {
  const meta = SLOT_STATE_META[slot.state]
  const capacity =
    slot.mig_profile ??
    (slot.capacity_mb != null ? `${Math.round(slot.capacity_mb / 1024)} GB` : "")

  return (
    <div
      className={cn(
        "flex min-w-20 flex-col items-center rounded-md border px-3 py-1.5",
        slot.state === "FREE" && "border-slot-free/50",
        slot.state === "RUNNING" && "border-slot-running/60 bg-slot-running/10",
        slot.state === "OFFLINE" && "border-dashed opacity-60",
        (slot.state === "RESERVED" || slot.state === "DEPLOYING" || slot.state === "STOPPING") &&
          "border-slot-reserved/60 bg-slot-reserved/10",
        slot.state === "FAILED" && "border-slot-failed/60 bg-slot-failed/10",
      )}
      title={`${meta.label} · ${slot.slot_type}`}
    >
      <span className="font-mono text-xs font-medium">{slot.name}</span>
      <span className={cn("text-[10px]", meta.textClass)}>{capacity || meta.label}</span>
    </div>
  )
}
