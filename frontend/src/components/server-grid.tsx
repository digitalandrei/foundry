import { Link, useNavigate } from "@tanstack/react-router"
import { useDroppable } from "@dnd-kit/core"
import { CheckCircle2Icon, ServerOffIcon, TriangleAlertIcon } from "lucide-react"

import { EmptyState } from "@/components/empty-state"
import { Skeleton } from "@/components/ui/skeleton"
import { useDeployments, useLatestMetrics } from "@/hooks/use-deployments"
import { useServers } from "@/hooks/use-servers"
import { formatLoad, formatMemGb } from "@/lib/format"
import { occupantsBySlot, slotDeployability } from "@/lib/slots"
import { DEPLOYMENT_STATE_META, SERVER_STATUS_META, SLOT_STATE_META } from "@/lib/states"
import type {
  DeploymentSummary,
  DropSlotData,
  GpuSummary,
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
  occupants: Map<string, DeploymentSummary>
  sample: MetricsSample | undefined
  onSelect: (deploymentId: string) => void
}) {
  const meta = SERVER_STATUS_META[server.status]
  const offline = server.status === "OFFLINE"
  // Per-server host telemetry (CPU load / cores · used / total RAM).
  // Hidden when offline — the last sample would be stale.
  const host = !offline ? sample?.host : undefined

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
        <span className="ml-auto flex items-center gap-x-3 text-xs text-muted-foreground tabular-nums">
          {host ? (
            <span
              className="flex items-center gap-x-2"
              title="Host 1-minute load average / logical cores · used / total RAM"
            >
              <span>
                <span className="text-[10px] uppercase opacity-70">cpu </span>
                {formatLoad(host.load_avg_1m, host.cpu_cores)}
              </span>
              <span>
                <span className="text-[10px] uppercase opacity-70">mem </span>
                {formatMemGb(host.mem_used_mb, host.mem_total_mb)}
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
  // A FREE slot shows nothing even if a now-dismissable FAILED
  // deployment still references it (auto-heal: failures free the slot).
  const shown = slot.state === "FREE" ? undefined : occupant
  // External (non-Foundry) container on this device — only surfaced
  // when Foundry has nothing here. A *running* one holds the GPU (not a
  // deploy target); a stopped one is shown but leaves the device free.
  const external = !shown ? slot.external : null
  const data: DropSlotData = {
    slotId: slot.id,
    slotName: slot.name,
    slotState: slot.state,
    serverId: server.id,
    serverName: server.name,
  }
  // Same eligibility the slot picker uses: FREE → deploy, RUNNING →
  // replace; needs an ONLINE server, Docker up, no external holder.
  const { deployable } = slotDeployability(slot, server, occupant)
  const { setNodeRef, isOver } = useDroppable({ id: slot.id, data, disabled: !deployable })
  const capacity =
    slot.mig_profile ??
    (slot.capacity_mb != null ? `${Math.round(slot.capacity_mb / 1024)} GB` : "")
  const usage = containerMetrics(sample, shown?.container_id ?? null)

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
      title={`${meta.label} · ${slot.slot_type}${slot.state === "RUNNING" ? " — drop to replace, click for details" : occupant ? " — click for details" : external ? " — GPU in use by a non-Foundry container" : ""}`}
    >
      {/* Slot number, occupant name, and status all on one line. */}
      <span className="flex items-baseline justify-between gap-2">
        <span className="flex min-w-0 items-center gap-1.5">
          <span className="shrink-0 font-mono text-xs font-medium">SLOT {slot.name}</span>
          {shown ? (
            <span
              className="flex min-w-0 items-center gap-1 truncate text-xs font-medium"
              title={shown.image_ref}
            >
              <StatusDot
                running={shown.state === "RUNNING"}
                className={DEPLOYMENT_STATE_META[shown.state].dotClass}
              />
              <span className="truncate">{shown.name}</span>
              <span className="shrink-0 text-[10px] font-normal text-muted-foreground">
                {shown.state === "RUNNING" ? "running" : shown.state.toLowerCase().replace(/_/g, " ")}
              </span>
            </span>
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
      {/* Second line only when there's live detail: deploy progress, or the
          container's own CPU/mem usage, or the external image. Run-state
          moved inline above. */}
      {shown && (shown.status_detail || usage) ? (
        <span
          className="truncate pl-3 font-mono text-[10px] text-muted-foreground tabular-nums"
          title={usage ? "Container CPU usage (cores) / allotted cores · used / limit RAM" : undefined}
        >
          {shown.status_detail
            ? shown.status_detail
            : `${formatLoad(usage!.cpu_pct / 100, usage!.cpu_cores)} · ${formatMemGb(usage!.mem_used_mb, usage!.mem_limit_mb)}`}
        </span>
      ) : external ? (
        <span className="truncate pl-3 font-mono text-[10px] text-muted-foreground" title={external.image}>
          {external.image}
        </span>
      ) : null}
    </div>
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
