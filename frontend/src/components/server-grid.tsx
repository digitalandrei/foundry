import { Link, useNavigate } from "@tanstack/react-router"
import { useDroppable } from "@dnd-kit/core"
import { BoxesIcon, CheckCircle2Icon, ServerOffIcon, TriangleAlertIcon } from "lucide-react"

import { EmptyState } from "@/components/empty-state"
import { Skeleton } from "@/components/ui/skeleton"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { useDeployments, useLatestMetrics } from "@/hooks/use-deployments"
import { useServerGroups } from "@/hooks/use-gpu-groups"
import { useServers } from "@/hooks/use-servers"
import { formatLoad, formatMemGb } from "@/lib/format"
import { occupantsBySlot, slotDeployability } from "@/lib/slots"
import { DEPLOYMENT_STATE_META, SERVER_STATUS_META, SLOT_STATE_META } from "@/lib/states"
import type {
  DeploymentSummary,
  DropGroupData,
  DropSlotData,
  GpuGroup,
  GpuSummary,
  HostMetrics,
  MetricsSample,
  ServerSummary,
  SlotSummary,
} from "@/lib/types"
import { cn } from "@/lib/utils"

/** Container ids appear both short (12) and full (64) — match either way. */
function containerMetrics(sample: MetricsSample | undefined, containerId: string | null) {
  if (!sample || !containerId) return undefined
  return sample.containers.find(
    (c) => c.container_id.startsWith(containerId) || containerId.startsWith(c.container_id),
  )
}

// Host-usage warning thresholds (operator request). Amber "heads-up"
// badges, not hard failures: disk/mem near-full cause real failures
// (pull/write errors, OOM kills); CPU saturated (1-min load ≥ cores)
// just means contention.
const DISK_WARN_PCT = 90
const MEM_WARN_PCT = 90
const CPU_WARN_RATIO = 1.0

function hostAlerts(host: HostMetrics) {
  const diskPct = (host.disk_used_gb / Math.max(1, host.disk_total_gb)) * 100
  const memPct = (host.mem_used_mb / Math.max(1, host.mem_total_mb)) * 100
  const cpuRatio = host.load_avg_1m / Math.max(1, host.cpu_cores)
  return {
    disk: diskPct >= DISK_WARN_PCT,
    mem: memPct >= MEM_WARN_PCT,
    cpu: cpuRatio >= CPU_WARN_RATIO,
    diskPct,
    memPct,
    cpuRatio,
  }
}
type HostAlertFlags = ReturnType<typeof hostAlerts>

/** "Servers & GPU Slots" — the dashboard grid (docs/UI-DESIGN.md § 2).
 * One row per server; GPU cells split the full row width; slot chips
 * stretch, carry the occupant + live usage, and open the detail dialog. */
export function ServerGrid() {
  const servers = useServers()
  const deployments = useDeployments()
  const metrics = useLatestMetrics()
  const navigate = useNavigate()
  // Clicking an occupied slot opens the dedicated deployment page.
  const openDeployment = (deploymentId: string) =>
    navigate({ to: "/deployments/$deploymentId", params: { deploymentId } })

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
  const occupants = occupantsBySlot(deployments.data)
  const samples = new Map((metrics.data?.servers ?? []).map((m) => [m.server_id, m.sample]))

  return (
    <div className="flex flex-col gap-3">
      {servers.data.map((server) => (
        <ServerRow
          key={server.id}
          server={server}
          occupants={occupants}
          sample={samples.get(server.id)}
          onSelect={openDeployment}
        />
      ))}
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
  occupants: Map<string, DeploymentSummary[]>
  sample: MetricsSample | undefined
  onSelect: (deploymentId: string) => void
}) {
  const meta = SERVER_STATUS_META[server.status]
  const offline = server.status === "OFFLINE"
  // Per-server host telemetry (CPU load / cores · used / total RAM ·
  // disk). Hidden when offline — the last sample would be stale.
  const host = !offline ? sample?.host : undefined
  const alerts = host ? hostAlerts(host) : null

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
        {server.hostname && server.hostname !== server.name ? (
          <span className="text-sm text-muted-foreground">{server.hostname}</span>
        ) : null}
        {server.os_version ? (
          <span className="text-xs text-muted-foreground">{server.os_version}</span>
        ) : null}
        <DockerStatus ok={server.docker_ok} />
        <NginxStatus status={server.nginx_status} ready={server.app_publishing_ready} />
        {alerts ? <HostAlerts alerts={alerts} /> : null}
        <span className="ml-auto flex items-center gap-x-3 text-xs text-muted-foreground tabular-nums">
          {host ? (
            <span
              className="flex items-center gap-x-2"
              title="Host 1-minute load average / logical cores · used / total RAM · root-fs disk used / total"
            >
              <span className={cn(alerts?.cpu && "font-medium text-slot-reserved")}>
                <span className="text-[10px] uppercase opacity-70">cpu </span>
                {formatLoad(host.load_avg_1m, host.cpu_cores)}
              </span>
              <span className={cn(alerts?.mem && "font-medium text-slot-reserved")}>
                <span className="text-[10px] uppercase opacity-70">mem </span>
                {formatMemGb(host.mem_used_mb, host.mem_total_mb)}
              </span>
              <span className={cn(alerts?.disk && "font-medium text-slot-reserved")}>
                <span className="text-[10px] uppercase opacity-70">disk </span>
                {host.disk_used_gb.toFixed(0)} / {host.disk_total_gb.toFixed(0)} GB
              </span>
            </span>
          ) : null}
          <span className="shrink-0">
            {server.gpus.length} GPU{server.gpus.length === 1 ? "" : "s"}
          </span>
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

      <GroupSection server={server} occupants={occupants} sample={sample} onSelect={onSelect} />
    </div>
  )
}

/** Per-server GPU-groups deploy affordance (overlay model — the member
 * GPUs keep their own cells above; a group is a *separate* target that
 * runs one container across all of them). Each group renders as its own
 * box (like a GPU cell) listing its member GPUs as slots, so the operator
 * sees exactly which GPUs a deploy will use. Hidden when the server
 * defines no groups. */
function GroupSection({
  server,
  occupants,
  sample,
  onSelect,
}: {
  server: ServerSummary
  occupants: Map<string, DeploymentSummary[]>
  sample: MetricsSample | undefined
  onSelect: (deploymentId: string) => void
}) {
  const groups = useServerGroups(server.id)
  const deployments = useDeployments()
  if (!groups.data || groups.data.length === 0) return null

  // Live group deploy → the box navigates to it instead of deploying.
  const activeByGroup = new Map<string, DeploymentSummary>()
  for (const d of deployments.data ?? []) {
    if (d.gpu_group_id && d.state !== "REMOVED" && d.state !== "REPLACED") {
      const cur = activeByGroup.get(d.gpu_group_id)
      if (!cur || d.created_at > cur.created_at) activeByGroup.set(d.gpu_group_id, d)
    }
  }

  return (
    <div className="mt-3">
      <p className="mb-1.5 inline-flex items-center gap-1 text-xs font-medium text-muted-foreground">
        <BoxesIcon className="size-3.5" aria-hidden /> Groups — one container across every member GPU
      </p>
      <div className="grid gap-2 [grid-template-columns:repeat(auto-fit,minmax(280px,1fr))]">
        {groups.data.map((group) => (
          <GroupCell
            key={group.id}
            group={group}
            server={server}
            occupants={occupants}
            active={activeByGroup.get(group.id)}
            sample={sample}
            onSelect={onSelect}
          />
        ))}
      </div>
    </div>
  )
}

/** A group rendered like a GPU cell: a header (name · N GPUs · combined
 * VRAM · status) over its member GPUs listed as slot rows. The whole box
 * is the drop/tap target while deployable (backend `deployable`); a busy
 * group shows its `busy_reason`; a live group deploy clicks through to its
 * deployment (and each member row reads "occupied by group"). */
function GroupCell({
  group,
  server,
  occupants,
  active,
  sample,
  onSelect,
}: {
  group: GpuGroup
  server: ServerSummary
  occupants: Map<string, DeploymentSummary[]>
  active: DeploymentSummary | undefined
  sample: MetricsSample | undefined
  onSelect: (deploymentId: string) => void
}) {
  const vramGb = Math.round(group.combined_vram_mb / 1024)
  const gpuById = new Map(server.gpus.map((gpu) => [gpu.id, gpu]))
  const data: DropGroupData = {
    kind: "group",
    groupId: group.id,
    groupName: group.name,
    serverId: server.id,
    serverName: server.name,
    memberCount: group.gpu_ids.length,
    vramMb: group.combined_vram_mb,
  }
  // Don't recompute deployability — the backend already folded the group's
  // cap, member occupancy, online + eligibility into `deployable` +
  // `busy_reason`. A multi-use group stays droppable while below its cap
  // even with a live deploy (hence not `&& !active`).
  const multi = group.max_occupants > 1
  const droppable = group.deployable
  const { setNodeRef, isOver } = useDroppable({ id: `group:${group.id}`, data, disabled: !droppable })

  return (
    <div
      ref={setNodeRef}
      onClick={active ? () => onSelect(active.id) : undefined}
      role={active ? "button" : undefined}
      title={
        active
          ? `Running ${active.name} — click for details`
          : droppable
            ? "Drop a container here to deploy one container across every member GPU"
            : (group.busy_reason ?? undefined)
      }
      className={cn(
        "flex flex-col rounded-md border bg-card/50 p-2 transition-colors",
        active && "cursor-pointer border-slot-running/60 bg-slot-running/10 hover:bg-accent/40",
        !active && droppable && "border-slot-free/50",
        !active && !droppable && "border-dashed opacity-70",
        isOver && droppable && "border-2 border-dashed border-slot-free bg-slot-free/15",
      )}
    >
      {/* Header — mirrors the GPU-cell header line. */}
      <p className="mb-1.5 flex flex-wrap items-baseline gap-x-2 gap-y-1 text-xs font-medium text-muted-foreground">
        <span className="inline-flex items-center gap-1 font-mono font-medium text-foreground/80">
          <BoxesIcon className="size-3.5" aria-hidden /> GROUP SLOT {group.name}
        </span>
        <span>
          {group.gpu_ids.length} GPUs · {vramGb} GB
        </span>
        <span className="ml-auto flex items-center gap-2 font-normal">
          {multi ? (
            <span className="tabular-nums" title="single/multi-use — set in server config">
              {group.occupants} / {group.max_occupants}
            </span>
          ) : null}
          {group.deployable ? (
            <span className="text-slot-free">{multi ? "drop to deploy" : "Free · drop to deploy"}</span>
          ) : group.busy_reason ? (
            <span className="text-slot-reserved" title={group.busy_reason}>
              {group.busy_reason}
            </span>
          ) : null}
        </span>
      </p>

      {/* Member GPUs, listed as slots (what this group will occupy). */}
      <div className="flex flex-col gap-1.5">
        {group.gpu_ids.map((gpuId) => {
          const gpu = gpuById.get(gpuId)
          if (!gpu) {
            return (
              <div
                key={gpuId}
                className="rounded-md border border-dashed px-3 py-1.5 text-[11px] text-muted-foreground"
              >
                a member GPU is not in the latest inventory
              </div>
            )
          }
          const fullSlot = gpu.slots.find((s) => s.slot_type === "FULL_GPU")
          const occ = fullSlot ? (occupants.get(fullSlot.id) ?? []) : []
          // While the group runs, every member shows the group occupant;
          // otherwise surface any individual occupant (names the blocker).
          const shown = active ?? occ[0]
          const usage = containerMetrics(sample, shown?.container_id ?? null)
          const gpuVram = gpu.memory_mb != null ? `${Math.round(gpu.memory_mb / 1024)} GB` : "? GB"
          return (
            <div
              key={gpuId}
              className={cn(
                "flex flex-col gap-0.5 rounded-md border px-3 py-1.5",
                shown ? "border-slot-running/40 bg-slot-running/5" : "border-border",
              )}
            >
              <span className="flex items-baseline justify-between gap-2">
                <span className="flex min-w-0 items-center gap-1.5">
                  <span className="shrink-0 font-mono text-xs font-medium">
                    GPU {gpu.index}
                    {fullSlot ? ` · SLOT ${fullSlot.name}` : ""}
                  </span>
                  {shown ? (
                    <OccupantName deployment={shown} />
                  ) : gpu.model ? (
                    <span className="truncate text-[11px] text-muted-foreground">{gpu.model}</span>
                  ) : null}
                </span>
                <span className="shrink-0 text-[10px] text-muted-foreground tabular-nums">{gpuVram}</span>
              </span>
              {shown && (shown.status_detail || usage) ? (
                <span className="truncate pl-3 font-mono text-[10px] text-muted-foreground tabular-nums">
                  {shown.status_detail
                    ? shown.status_detail
                    : `${formatLoad(usage!.cpu_pct / 100, usage!.cpu_cores)} · ${formatMemGb(usage!.mem_used_mb, usage!.mem_limit_mb)}`}
                </span>
              ) : null}
            </div>
          )
        })}
      </div>
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
  occupants: Map<string, DeploymentSummary[]>
  sample: MetricsSample | undefined
  onSelect: (deploymentId: string) => void
}) {
  // Per-GPU silicon telemetry; NVML cannot attribute per MIG slice, so
  // it lives on the GPU header (container stats live on the chips).
  const g = sample?.gpus.find((m) => m.uuid === gpu.gpu_uuid)

  return (
    <div className="rounded-md border bg-card/50 p-2">
      <p className="mb-1.5 flex flex-wrap items-baseline gap-x-2 gap-y-1 text-xs font-medium text-muted-foreground">
        <span>
          GPU {gpu.index} {gpu.model ? `(${gpu.model})` : ""}
        </span>
        {gpu.mig_enabled ? (
          <span className="text-slot-free">MIG</span>
        ) : (
          <span>No MIG</span>
        )}
        <GpuGroupChips groups={gpu.groups} />
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
            occupants={occupants.get(slot.id) ?? []}
            sample={sample}
            onSelect={onSelect}
          />
        ))}
      </div>
    </div>
  )
}

/** Per-GPU group-membership evidence (overlap model): a GPU may sit in
 * several groups, and that overlap must be visible (docs/UI-DESIGN.md).
 * Renders `grp A, B`; a tooltip spells the names out in full. */
function GpuGroupChips({ groups }: { groups: GpuSummary["groups"] }) {
  if (groups.length === 0) return null
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="inline-flex items-center gap-1 rounded-sm bg-muted px-1.5 font-normal text-foreground/70">
          <BoxesIcon className="size-3" aria-hidden />
          grp {groups.map((gr) => gr.name).join(", ")}
        </span>
      </TooltipTrigger>
      <TooltipContent>
        In {groups.length} group{groups.length === 1 ? "" : "s"}: {groups.map((gr) => gr.name).join(", ")}
      </TooltipContent>
    </Tooltip>
  )
}

/** Drop target. Single-use: FREE → deploy; RUNNING → replace (confirmed in
 * the dialog); other states inactive. Multi-use (`max_occupants > 1`):
 * shows `k / N`, stacks each co-tenant's chip, and stays a drop target
 * while `k < N` (no "replace"). An occupant chip clicks through to its
 * deployment detail page (docs/UI-DESIGN.md). */
function SlotChip({
  slot,
  server,
  occupants,
  sample,
  onSelect,
}: {
  slot: SlotSummary
  server: ServerSummary
  occupants: DeploymentSummary[]
  sample: MetricsSample | undefined
  onSelect: (deploymentId: string) => void
}) {
  const multi = slot.max_occupants > 1
  const meta = SLOT_STATE_META[slot.state]
  // A FREE single-use slot shows nothing even if a now-dismissable FAILED
  // deployment still references it (auto-heal: failures free the slot).
  // Multi-use occupancy is the live count, independent of the state enum.
  const active = multi ? occupants : slot.state === "FREE" ? [] : occupants.slice(0, 1)
  // External (non-Foundry) container on this device — only surfaced when
  // Foundry holds nothing. A *running* one holds the GPU; a stopped one
  // shows but leaves the device free.
  const external = active.length === 0 ? slot.external : null
  const data: DropSlotData = {
    kind: "slot",
    slotId: slot.id,
    slotName: slot.name,
    slotState: slot.state,
    serverId: server.id,
    serverName: server.name,
  }
  const { deployable } = slotDeployability(slot, server, occupants)
  const { setNodeRef, isOver } = useDroppable({ id: slot.id, data, disabled: !deployable })
  const capacity =
    slot.mig_profile ??
    (slot.capacity_mb != null ? `${Math.round(slot.capacity_mb / 1024)} GB` : "")

  // Multi-use: a `k / N` header + a stacked chip per co-tenant (compact
  // "+N more" past two). Each chip routes to its own deployment.
  if (multi) {
    const VISIBLE = 2
    const visible = active.slice(0, VISIBLE)
    const overflow = active.length - visible.length
    return (
      <div
        ref={setNodeRef}
        className={cn(
          "flex min-w-32 flex-1 flex-col gap-1 rounded-md border px-3 py-2 transition-colors",
          slot.state === "OFFLINE" && "border-dashed opacity-60",
          active.length > 0 ? "border-slot-running/40" : !external && "border-slot-free/50",
          external && "border-foreground/25 bg-muted/40",
          isOver && deployable && "border-2 border-dashed border-slot-free bg-slot-free/15",
        )}
        title={`Multi-use slot · soft sharing without VRAM isolation · ${active.length}/${slot.max_occupants} in use`}
      >
        <span className="flex items-baseline justify-between gap-2">
          <span className="font-mono text-xs font-medium">SLOT {slot.name}</span>
          <span className="shrink-0 text-[10px] text-muted-foreground tabular-nums">
            {capacity ? `${capacity} · ` : ""}
            {external ? "External" : `${active.length} / ${slot.max_occupants}`}
          </span>
        </span>
        {visible.map((d) => (
          <OccupantLine key={d.id} deployment={d} sample={sample} onSelect={onSelect} />
        ))}
        {overflow > 0 ? (
          <span className="pl-3 text-[10px] text-muted-foreground">+{overflow} more</span>
        ) : null}
        {active.length === 0 && external ? <ExternalLine external={external} /> : null}
      </div>
    )
  }

  // Single-use: the original one-occupant rendering.
  const shown = active[0]
  return (
    <div
      ref={setNodeRef}
      onClick={shown ? () => onSelect(shown.id) : undefined}
      role={shown ? "button" : undefined}
      className={cn(
        "flex min-w-32 flex-1 flex-col gap-0.5 rounded-md border px-3 py-2 transition-colors",
        shown && "cursor-pointer hover:bg-accent/40",
        slot.state === "FREE" && !external && "border-slot-free/50",
        external && "border-foreground/25 bg-muted/40",
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
      title={`${meta.label} · ${slot.slot_type}${slot.state === "RUNNING" ? " — drop to replace, click for details" : shown ? " — click for details" : external ? " — GPU in use by a non-Foundry container" : ""}`}
    >
      <span className="flex items-baseline justify-between gap-2">
        <span className="flex min-w-0 items-center gap-1.5">
          <span className="shrink-0 font-mono text-xs font-medium">SLOT {slot.name}</span>
          {shown ? (
            <OccupantName deployment={shown} />
          ) : external ? (
            <span
              className="flex min-w-0 items-center gap-1 truncate text-xs font-medium"
              title={external.image}
            >
              <StatusDot running={external.running} />
              <span className="truncate">{external.name}</span>
              <span className="shrink-0 text-[10px] font-normal text-muted-foreground">
                {external.running ? "running" : "stopped"}
              </span>
            </span>
          ) : null}
        </span>
        <span className={cn("shrink-0 text-[10px]", external ? "text-muted-foreground" : meta.textClass)}>
          {capacity ? `${capacity} · ` : ""}
          {external ? "External" : meta.label}
        </span>
      </span>
      {shown ? (
        <OccupantUsageLine deployment={shown} sample={sample} />
      ) : external ? (
        <span className="truncate pl-3 font-mono text-[10px] text-muted-foreground" title={external.image}>
          {external.image}
        </span>
      ) : null}
    </div>
  )
}

/** Occupant name + run-state, with a group deploy spelled out so every
 * member cell reads "occupied by group · {name}" consistently. */
function OccupantName({ deployment }: { deployment: DeploymentSummary }) {
  const grouped = deployment.gpu_group_id != null
  return (
    <span
      className="flex min-w-0 items-center gap-1 truncate text-xs font-medium"
      title={grouped ? `Group ${deployment.group_name} — ${deployment.image_ref}` : deployment.image_ref}
    >
      <StatusDot
        running={deployment.state === "RUNNING"}
        className={DEPLOYMENT_STATE_META[deployment.state].dotClass}
      />
      <span className="truncate">{deployment.name}</span>
      <span className="shrink-0 text-[10px] font-normal text-muted-foreground">
        {grouped
          ? `group · ${deployment.group_name}`
          : deployment.state === "RUNNING"
            ? "running"
            : deployment.state.toLowerCase().replace(/_/g, " ")}
      </span>
    </span>
  )
}

/** Second line for a single-use occupant: deploy progress or live CPU/mem. */
function OccupantUsageLine({
  deployment,
  sample,
}: {
  deployment: DeploymentSummary
  sample: MetricsSample | undefined
}) {
  const usage = containerMetrics(sample, deployment.container_id)
  if (!deployment.status_detail && !usage) return null
  return (
    <span
      className="truncate pl-3 font-mono text-[10px] text-muted-foreground tabular-nums"
      title={usage ? "Container CPU usage (cores) / allotted cores · used / limit RAM" : undefined}
    >
      {deployment.status_detail
        ? deployment.status_detail
        : `${formatLoad(usage!.cpu_pct / 100, usage!.cpu_cores)} · ${formatMemGb(usage!.mem_used_mb, usage!.mem_limit_mb)}`}
    </span>
  )
}

/** One co-tenant row inside a multi-use slot: name + run-state + usage,
 * clickable through to the deployment. */
function OccupantLine({
  deployment,
  sample,
  onSelect,
}: {
  deployment: DeploymentSummary
  sample: MetricsSample | undefined
  onSelect: (deploymentId: string) => void
}) {
  const usage = containerMetrics(sample, deployment.container_id)
  return (
    <button
      type="button"
      onClick={() => onSelect(deployment.id)}
      className="flex min-w-0 flex-col items-start gap-0.5 rounded-sm px-1 py-0.5 text-left transition-colors hover:bg-accent/40"
    >
      <OccupantName deployment={deployment} />
      {deployment.status_detail || usage ? (
        <span className="truncate pl-3 font-mono text-[10px] text-muted-foreground tabular-nums">
          {deployment.status_detail
            ? deployment.status_detail
            : `${formatLoad(usage!.cpu_pct / 100, usage!.cpu_cores)} · ${formatMemGb(usage!.mem_used_mb, usage!.mem_limit_mb)}`}
        </span>
      ) : null}
    </button>
  )
}

/** External (non-Foundry) container line inside a multi-use slot. */
function ExternalLine({ external }: { external: NonNullable<SlotSummary["external"]> }) {
  return (
    <span className="flex min-w-0 items-center gap-1 truncate pl-1 text-xs font-medium" title={external.image}>
      <StatusDot running={external.running} />
      <span className="truncate">{external.name}</span>
      <span className="shrink-0 text-[10px] font-normal text-muted-foreground">
        {external.running ? "running" : "stopped"}
      </span>
    </span>
  )
}

/** Host-usage warnings (operator request): amber badges shown only when a
 * threshold trips — disk/RAM near-full or CPU saturated. Quiet otherwise;
 * the live numbers already sit in the host readout on the right. */
function HostAlerts({ alerts }: { alerts: HostAlertFlags }) {
  const badges = [
    alerts.disk && {
      key: "disk",
      label: `disk ${alerts.diskPct.toFixed(0)}%`,
      title:
        "Root filesystem is nearly full — image pulls and container writes will start failing. Free space on the server.",
    },
    alerts.mem && {
      key: "mem",
      label: `mem ${alerts.memPct.toFixed(0)}%`,
      title: "Host memory is nearly exhausted — new containers risk being OOM-killed.",
    },
    alerts.cpu && {
      key: "cpu",
      label: `cpu ${alerts.cpuRatio.toFixed(1)}×`,
      title:
        "1-minute load average exceeds the logical core count — the host is CPU-saturated and work will contend.",
    },
  ].filter(Boolean) as { key: string; label: string; title: string }[]

  return (
    <>
      {badges.map((b) => (
        <span
          key={b.key}
          className="inline-flex items-center gap-1 text-xs text-slot-reserved"
          title={b.title}
        >
          <TriangleAlertIcon className="size-3.5" aria-hidden />
          {b.label}
        </span>
      ))}
    </>
  )
}

/** Per-server Docker daemon health (same shape as nginx). Active → quiet
 * green; down → red "deploys blocked"; unknown (no snapshot yet) → silent. */
function DockerStatus({ ok }: { ok: boolean | null }) {
  if (ok === null) return null
  if (ok) {
    return (
      <span
        className="inline-flex items-center gap-1 text-xs text-slot-free"
        title="The Docker daemon is running — deployments are allowed."
      >
        <CheckCircle2Icon className="size-3.5" aria-hidden />
        docker: active
      </span>
    )
  }
  return (
    <span
      className="inline-flex items-center gap-1 text-xs text-slot-failed"
      title="The Docker daemon is not responding on this server. Start it (`sudo systemctl start docker`); deploys are blocked until it's back."
    >
      <TriangleAlertIcon className="size-3.5" aria-hidden />
      Docker stopped — deploys blocked
    </span>
  )
}

/** Per-server nginx / HTTP-S-publishing health. Healthy → a quiet green
 * "nginx: active"; otherwise a precise, actionable warning. */
function NginxStatus({ status, ready }: { status: string | null; ready: boolean | null }) {
  // No snapshot yet / pre-0.16 agent with unknown readiness → say nothing.
  if (status === null && ready === null) return null

  if (status === "READY" || (status === null && ready === true)) {
    return (
      <span className="inline-flex items-center gap-1 text-xs text-slot-free" title="nginx is active and configured — HTTP/S app publishing is ready.">
        <CheckCircle2Icon className="size-3.5" aria-hidden />
        nginx: active
      </span>
    )
  }

  const warn: Record<string, { label: string; title: string }> = {
    NGINX_MISSING: {
      label: "nginx not installed — HTTP/S off",
      title: "Install nginx, then run `sudo foundry-agent --setup-apps`.",
    },
    NGINX_OUTDATED: {
      label: "nginx too old — HTTP/S off",
      title: "nginx is older than 1.25.1 (no `http2` directive). Upgrade nginx on the server.",
    },
    NGINX_INACTIVE: {
      label: "nginx stopped — HTTP/S off",
      title: "nginx is installed but not running. `sudo systemctl enable --now nginx`.",
    },
    NOT_CONFIGURED: {
      label: "nginx up · not configured — HTTP/S off",
      title: "nginx is running but not set up for Foundry. Run `sudo foundry-agent --setup-apps`.",
    },
    TLS_MISSING: {
      label: "TLS cert missing — HTTP/S off",
      title: "nginx is ready but the wildcard certificate is missing. Install fullchain.pem + privkey.pem under /etc/foundry-agent/tls/.",
    },
  }
  const w = (status && warn[status]) || {
    label: "HTTP/S publishing off",
    title: "The agent reports HTTP/S app publishing is unavailable on this server.",
  }
  return (
    <span className="inline-flex items-center gap-1 text-xs text-slot-reserved" title={w.title}>
      <TriangleAlertIcon className="size-3.5" aria-hidden />
      {w.label}
    </span>
  )
}

/** Running = green, stopped/other = gray (or the passed Foundry-state
 * color). One source for the run-state dot on every slot occupant. */
function StatusDot({ running, className }: { running: boolean; className?: string }) {
  return (
    <span
      className={cn(
        "size-2 shrink-0 rounded-full",
        className ?? (running ? "bg-slot-running" : "bg-slot-offline"),
      )}
      title={running ? "running" : "stopped"}
      aria-hidden
    />
  )
}
