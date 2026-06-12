import { useState } from "react"
import { useDroppable } from "@dnd-kit/core"
import { Link } from "@tanstack/react-router"
import { ServerOffIcon } from "lucide-react"

import { EmptyState } from "@/components/empty-state"
import { SlotDetailDialog } from "@/components/slot-detail-dialog"
import { Skeleton } from "@/components/ui/skeleton"
import { useDeployments, useLatestMetrics } from "@/hooks/use-deployments"
import { useServers } from "@/hooks/use-servers"
import { SERVER_STATUS_META, SLOT_STATE_META } from "@/lib/states"
import type {
  DeploymentSummary,
  DropSlotData,
  GpuSummary,
  MetricsSample,
  ServerSummary,
  SlotSummary,
} from "@/lib/types"
import { cn } from "@/lib/utils"

/** Deployment states that occupy a slot (everything still on the host). */
const OCCUPYING = new Set([
  "PENDING",
  "VALIDATING",
  "PULLING_IMAGE",
  "CREATING_CONTAINER",
  "STARTING",
  "RUNNING",
  "STOPPING",
  "STOPPED",
  "RESTARTING",
  "REMOVING",
  "FAILED",
])

/** Container ids appear both short (12) and full (64) — match either way. */
function containerMetrics(sample: MetricsSample | undefined, containerId: string | null) {
  if (!sample || !containerId) return undefined
  return sample.containers.find(
    (c) => c.container_id.startsWith(containerId) || containerId.startsWith(c.container_id),
  )
}

/** "Servers & GPU Slots" — the dashboard grid (docs/UI-DESIGN.md § 2).
 * One row per server; GPU cells split the full row width; slot chips
 * stretch, carry the occupant + live usage, and open the detail dialog. */
export function ServerGrid() {
  const servers = useServers()
  const deployments = useDeployments()
  const metrics = useLatestMetrics()
  const [selected, setSelected] = useState<string | null>(null)

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

  // Latest occupant per slot (REMOVED/REPLACED no longer hold one).
  const occupants = new Map<string, DeploymentSummary>()
  for (const d of deployments.data ?? []) {
    if (!OCCUPYING.has(d.state)) continue
    const existing = occupants.get(d.slot_id)
    if (!existing || d.created_at > existing.created_at) occupants.set(d.slot_id, d)
  }
  const samples = new Map((metrics.data?.servers ?? []).map((m) => [m.server_id, m.sample]))

  return (
    <div className="flex flex-col gap-3">
      {servers.data.map((server) => (
        <ServerRow
          key={server.id}
          server={server}
          occupants={occupants}
          sample={samples.get(server.id)}
          onSelect={setSelected}
        />
      ))}
      <SlotDetailDialog deploymentId={selected} onClose={() => setSelected(null)} />
    </div>
  )
}

function ServerRow({
  server,
  occupants,
  sample,
  onSelect,
}: {
  server: ServerSummary
  occupants: Map<string, DeploymentSummary>
  sample: MetricsSample | undefined
  onSelect: (deploymentId: string) => void
}) {
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
        // GPUs split the full row width — 2 GPUs → 50/50, any count fits.
        <div className="mt-2 grid gap-2 [grid-template-columns:repeat(auto-fit,minmax(280px,1fr))]">
          {server.gpus.map((gpu) => (
            <GpuCell
              key={gpu.id}
              gpu={gpu}
              server={server}
              occupants={occupants}
              sample={sample}
              onSelect={onSelect}
            />
          ))}
        </div>
      )}
    </div>
  )
}

function GpuCell({
  gpu,
  server,
  occupants,
  sample,
  onSelect,
}: {
  gpu: GpuSummary
  server: ServerSummary
  occupants: Map<string, DeploymentSummary>
  sample: MetricsSample | undefined
  onSelect: (deploymentId: string) => void
}) {
  // Per-GPU silicon telemetry; NVML cannot attribute per MIG slice, so
  // it lives on the GPU header (container stats live on the chips).
  const g = sample?.gpus.find((m) => m.uuid === gpu.gpu_uuid)

  return (
    <div className="rounded-md border bg-card/50 p-2">
      <p className="mb-1.5 flex flex-wrap items-baseline gap-x-2 text-xs font-medium text-muted-foreground">
        <span>
          GPU {gpu.index} {gpu.model ? `(${gpu.model})` : ""}
        </span>
        {gpu.mig_enabled ? (
          <span className="text-slot-free">MIG</span>
        ) : (
          <span>No MIG</span>
        )}
        {g ? (
          <span className="ml-auto font-normal tabular-nums">
            {g.util_pct}% · {(g.mem_used_mb / 1024).toFixed(1)}/
            {gpu.memory_mb != null ? Math.round(gpu.memory_mb / 1024) : "?"} GB · {g.temperature_c}°C ·{" "}
            {Math.round(g.power_w)} W
          </span>
        ) : null}
      </p>
      <div className="flex flex-wrap gap-1.5">
        {gpu.slots.map((slot) => (
          <SlotChip
            key={slot.id}
            slot={slot}
            server={server}
            occupant={occupants.get(slot.id)}
            sample={sample}
            onSelect={onSelect}
          />
        ))}
      </div>
    </div>
  )
}

/** Drop target: FREE → deploy; RUNNING → replace (with confirmation in
 * the dialog). Other states don't activate (docs/UI-DESIGN.md). An
 * occupied chip clicks through to the deployment detail dialog. */
function SlotChip({
  slot,
  server,
  occupant,
  sample,
  onSelect,
}: {
  slot: SlotSummary
  server: ServerSummary
  occupant: DeploymentSummary | undefined
  sample: MetricsSample | undefined
  onSelect: (deploymentId: string) => void
}) {
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
  const usage = containerMetrics(sample, occupant?.container_id ?? null)

  return (
    <div
      ref={setNodeRef}
      onClick={occupant ? () => onSelect(occupant.id) : undefined}
      role={occupant ? "button" : undefined}
      className={cn(
        "flex min-w-32 flex-1 flex-col gap-0.5 rounded-md border px-3 py-2 transition-colors",
        occupant && "cursor-pointer hover:bg-accent/40",
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
      title={`${meta.label} · ${slot.slot_type}${slot.state === "RUNNING" ? " — drop to replace, click for details" : occupant ? " — click for details" : ""}`}
    >
      <span className="flex items-baseline justify-between gap-2">
        <span className="font-mono text-xs font-medium">{slot.name}</span>
        <span className={cn("text-[10px]", meta.textClass)}>
          {capacity ? `${capacity} · ` : ""}
          {meta.label}
        </span>
      </span>
      {occupant ? (
        <>
          <span className="truncate text-xs font-medium" title={occupant.image_ref}>
            {occupant.name}
          </span>
          <span className="truncate font-mono text-[10px] text-muted-foreground tabular-nums">
            {occupant.status_detail
              ? occupant.status_detail
              : usage
                ? `CPU ${usage.cpu_pct.toFixed(0)}% · MEM ${(usage.mem_used_mb / 1024).toFixed(1)}/${(usage.mem_limit_mb / 1024).toFixed(0)} GB`
                : occupant.state !== "RUNNING"
                  ? occupant.state.toLowerCase().replace(/_/g, " ")
                  : "—"}
          </span>
        </>
      ) : null}
    </div>
  )
}
