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
import {
  groupSlotPositions,
  gpuSlotPositions,
  occupantsByGroup,
  occupantsBySlot,
  slotDeployability,
  type GroupPosition,
  type SlotPosition,
} from "@/lib/slots"
import { DEPLOYMENT_STATE_META, SERVER_STATUS_META } from "@/lib/states"
import type {
  DeploymentSummary,
  DropGroupData,
  DropSlotData,
  GpuGroup,
  GpuSummary,
  HostMetrics,
  MetricsSample,
  ServerSummary,
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

  // Latest occupant per slot (REMOVED/REPLACED no longer hold one), plus
  // the group-level occupancy that drives the group slot chips.
  const occupants = occupantsBySlot(deployments.data)
  const groupOccupants = occupantsByGroup(deployments.data)
  const samples = new Map((metrics.data?.servers ?? []).map((m) => [m.server_id, m.sample]))

  return (
    <div className="flex flex-col gap-3">
      {servers.data.map((server) => (
        <ServerRow
          key={server.id}
          server={server}
          occupants={occupants}
          groupOccupants={groupOccupants}
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
  groupOccupants,
  sample,
  onSelect,
}: {
  server: ServerSummary
  occupants: Map<string, DeploymentSummary[]>
  groupOccupants: Map<string, DeploymentSummary[]>
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

      <GroupSection
        server={server}
        groupOccupants={groupOccupants}
        sample={sample}
        onSelect={onSelect}
      />
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
  groupOccupants,
  sample,
  onSelect,
}: {
  server: ServerSummary
  groupOccupants: Map<string, DeploymentSummary[]>
  sample: MetricsSample | undefined
  onSelect: (deploymentId: string) => void
}) {
  const groups = useServerGroups(server.id)
  if (!groups.data || groups.data.length === 0) return null

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
            groupOccupants={groupOccupants}
            sample={sample}
            onSelect={onSelect}
          />
        ))}
      </div>
    </div>
  )
}

/** A group rendered exactly like a GPU cell: a header (GROUP SLOT name ·
 * the member GPUs as badges beside it · combined resources) over its
 * `SLOT 1..N` deploy positions. Each free slot is its own drop/tap target
 * (deploying one container across every member GPU); an occupied slot
 * clicks through to its deployment. Deployability is the backend's
 * (`deployable` + `busy_reason`); positions = the group's `max_occupants`
 * (1 single-use, up to 4 multi-use). */
function GroupCell({
  group,
  server,
  groupOccupants,
  sample,
  onSelect,
}: {
  group: GpuGroup
  server: ServerSummary
  groupOccupants: Map<string, DeploymentSummary[]>
  sample: MetricsSample | undefined
  onSelect: (deploymentId: string) => void
}) {
  const vramGb = Math.round(group.combined_vram_mb / 1024)
  const gpuById = new Map(server.gpus.map((gpu) => [gpu.id, gpu]))

  return (
    <div className="flex flex-col rounded-md border bg-card/50 p-2">
      {/* Header — GROUP SLOT name, then the member GPUs as badges to its
          right (which GPUs a deploy spans; wraps to more lines when there
          isn't room), then the combined resources. */}
      <p className="mb-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs font-medium text-muted-foreground">
        <span className="inline-flex items-center gap-1 font-mono font-medium text-foreground/80">
          <BoxesIcon className="size-3.5" aria-hidden /> GROUP SLOT {group.name}
        </span>
        {group.gpu_ids.map((gpuId) => {
          const gpu = gpuById.get(gpuId)
          return (
            <span
              key={gpuId}
              title={gpu?.model ?? undefined}
              className="rounded border bg-background px-1.5 py-0.5 font-mono text-[11px] font-medium text-foreground/80"
            >
              GPU {gpu ? gpu.index : "?"}
            </span>
          )
        })}
        <span className="ml-auto font-normal tabular-nums">
          {group.gpu_ids.length} GPUs · {vramGb} GB
        </span>
      </p>

      {/* SLOT 1..N — the group's deploy positions, rendered like a GPU's
          slots. A free slot is a drop target; an occupied one clicks
          through to its deployment. */}
      <div className="flex flex-wrap gap-1.5">
        {groupSlotPositions(group, groupOccupants).map((position) => (
          <GroupSlotChip
            key={position.label}
            group={group}
            position={position}
            server={server}
            sample={sample}
            onSelect={onSelect}
          />
        ))}
      </div>
    </div>
  )
}

/** One deploy position ("SLOT n") on a group — the group-level twin of
 * `SlotChip`. A free slot drops onto the whole group (one container across
 * every member GPU) while the backend says the group is deployable; an
 * occupied slot clicks through to its deployment. Groups never "replace"
 * (a deploy needs every member free), so an occupied single-use slot is
 * click-through only. */
function GroupSlotChip({
  group,
  position,
  server,
  sample,
  onSelect,
}: {
  group: GpuGroup
  position: GroupPosition
  server: ServerSummary
  sample: MetricsSample | undefined
  onSelect: (deploymentId: string) => void
}) {
  const { label, occupant } = position
  // Don't recompute deployability — the backend folded the group's cap,
  // member occupancy, online + eligibility into `deployable`/`busy_reason`.
  // A free slot is droppable only while the group as a whole is.
  const droppable = occupant === undefined && group.deployable
  const data: DropGroupData = {
    kind: "group",
    groupId: group.id,
    groupName: group.name,
    serverId: server.id,
    serverName: server.name,
    memberCount: group.gpu_ids.length,
    vramMb: group.combined_vram_mb,
  }
  const { setNodeRef, isOver } = useDroppable({
    id: `group:${group.id}#${label}`,
    data,
    disabled: !droppable,
  })

  const tone = occupant
    ? occupant.state === "RUNNING"
      ? "running"
      : occupant.state === "FAILED"
        ? "failed"
        : "reserved"
    : group.deployable
      ? "free"
      : "blocked"

  return (
    <div
      ref={setNodeRef}
      onClick={occupant ? () => onSelect(occupant.id) : undefined}
      role={occupant ? "button" : undefined}
      className={cn(
        "flex min-w-32 flex-1 flex-col gap-0.5 rounded-md border px-3 py-2 transition-colors",
        occupant && "cursor-pointer hover:bg-accent/40",
        tone === "free" && "border-slot-free/50",
        tone === "blocked" && "border-dashed opacity-70",
        tone === "running" && "border-slot-running/60 bg-slot-running/10",
        tone === "reserved" && "border-slot-reserved/60 bg-slot-reserved/10",
        tone === "failed" && "border-slot-failed/60 bg-slot-failed/10",
        isOver && droppable && "border-2 border-dashed border-slot-free bg-slot-free/15",
      )}
      title={
        occupant
          ? "click for details"
          : group.deployable
            ? "free — drop to deploy one container across every member GPU"
            : (group.busy_reason ?? undefined)
      }
    >
      <span className="flex items-baseline justify-between gap-2">
        <span className="flex min-w-0 items-center gap-1.5">
          <span className="shrink-0 font-mono text-xs font-medium">SLOT {label}</span>
          {occupant ? <OccupantName deployment={occupant} /> : null}
        </span>
        {occupant ? null : (
          <span
            className={cn("shrink-0 text-[10px]", group.deployable ? "text-slot-free" : "text-muted-foreground")}
            title={group.deployable ? undefined : (group.busy_reason ?? undefined)}
          >
            {group.deployable ? "Free" : "Busy"}
          </span>
        )}
      </span>
      {occupant ? <OccupantUsageLine deployment={occupant} sample={sample} /> : null}
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
        {gpu.mig_enabled ? <span className="text-slot-free">MIG</span> : null}
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
        {gpuSlotPositions(gpu, occupants).map((position) => (
          <SlotChip
            key={`${position.slot.id}#${position.label}`}
            position={position}
            server={server}
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

/** One deploy position ("SLOT n") in a GPU cell. A slot exposes
 * `max_occupants` positions; the i-th occupant fills the i-th, the rest
 * are free capacity. A free position is a deploy drop target; an occupied
 * single-use slot is a replace target; any other occupant clicks through
 * to its deployment. GPU memory is omitted — it sits on the GPU header
 * above (docs/UI-DESIGN.md). */
function SlotChip({
  position,
  server,
  sample,
  onSelect,
}: {
  position: SlotPosition
  server: ServerSummary
  sample: MetricsSample | undefined
  onSelect: (deploymentId: string) => void
}) {
  const { slot, label, occupant, occupants, firstOfSlot } = position
  const { deployable, replace } = slotDeployability(slot, server, occupants)
  // External (non-Foundry) holder is surfaced once, on the slot's first
  // position, and only when Foundry holds nothing.
  const external = firstOfSlot && occupants.length === 0 ? slot.external : null
  // This position replaces only when it holds the running occupant of a
  // single-use slot; a free position is a fresh deploy. `deployable`
  // already accounts for a *running* external holder (a stopped one
  // leaves the device free), so don't re-block on `external` here.
  const isReplace = occupant !== undefined && replace && slot.max_occupants === 1
  const droppable = occupant === undefined ? deployable : isReplace
  const data: DropSlotData = {
    kind: "slot",
    slotId: slot.id,
    slotName: label,
    slotState: slot.state,
    serverId: server.id,
    serverName: server.name,
    replace: isReplace,
  }
  const { setNodeRef, isOver } = useDroppable({
    id: `${slot.id}#${label}`,
    data,
    disabled: !droppable,
  })

  // Tone follows what fills the position — a free position on a busy
  // multi-use slot still looks free.
  const tone = occupant
    ? occupant.state === "RUNNING"
      ? "running"
      : occupant.state === "FAILED"
        ? "failed"
        : "reserved"
    : external
      ? "external"
      : slot.state === "OFFLINE"
        ? "offline"
        : "free"

  return (
    <div
      ref={setNodeRef}
      onClick={occupant ? () => onSelect(occupant.id) : undefined}
      role={occupant ? "button" : undefined}
      className={cn(
        "flex min-w-32 flex-1 flex-col gap-0.5 rounded-md border px-3 py-2 transition-colors",
        occupant && "cursor-pointer hover:bg-accent/40",
        tone === "free" && "border-slot-free/50",
        tone === "external" && "border-foreground/25 bg-muted/40",
        tone === "running" && "border-slot-running/60 bg-slot-running/10",
        tone === "reserved" && "border-slot-reserved/60 bg-slot-reserved/10",
        tone === "failed" && "border-slot-failed/60 bg-slot-failed/10",
        tone === "offline" && "border-dashed opacity-60",
        isOver && !isReplace && "border-2 border-dashed border-slot-free bg-slot-free/15",
        isOver && isReplace && "border-2 border-dashed border-slot-reserved bg-slot-reserved/15",
      )}
      title={
        isReplace
          ? "drop to replace, click for details"
          : occupant
            ? "click for details"
            : external
              ? "GPU in use by a non-Foundry container"
              : "free — drop to deploy"
      }
    >
      <span className="flex items-baseline justify-between gap-2">
        <span className="flex min-w-0 items-center gap-1.5">
          <span className="shrink-0 font-mono text-xs font-medium">SLOT {label}</span>
          {occupant ? (
            <OccupantName deployment={occupant} />
          ) : external ? (
            <span className="flex min-w-0 items-center gap-1 truncate text-xs font-medium" title={external.image}>
              <StatusDot running={external.running} />
              <span className="truncate">{external.name}</span>
              <span className="shrink-0 text-[10px] font-normal text-muted-foreground">
                {external.running ? "running" : "stopped"}
              </span>
            </span>
          ) : null}
        </span>
        {occupant ? null : (
          <span className={cn("shrink-0 text-[10px]", external ? "text-muted-foreground" : "text-slot-free")}>
            {external ? "External" : "Free"}
          </span>
        )}
      </span>
      {occupant ? (
        <OccupantUsageLine deployment={occupant} sample={sample} />
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
