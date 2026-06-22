import { MetricSparkline } from "@/components/metric-sparkline"
import { useServerMetrics } from "@/hooks/use-servers"
import { formatLoad, formatMemGb, formatSize } from "@/lib/format"
import type { GpuSummary, MetricsPoint, ServerSummary } from "@/lib/types"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"

/** Reusable telemetry building blocks shared by the per-server detail page
 * and the fleet-wide Telemetry tab, so the two never drift. Host metric
 * cards, per-GPU graphs, and per-MIG-slice memory all live here. */

function series(
  points: MetricsPoint[] | undefined,
  pick: (s: MetricsPoint["sample"]) => number,
): { ts: string; value: number }[] {
  return (points ?? []).map((p) => ({ ts: p.sampled_at, value: pick(p.sample) }))
}

function MetricCard({
  title,
  value,
  points,
  unit,
  max,
}: {
  title: string
  value: string
  points: { ts: string; value: number }[]
  unit: string
  max?: number
}) {
  return (
    <Card className="gap-2 py-4">
      <CardHeader className="py-0">
        <CardTitle className="flex items-baseline justify-between text-sm font-medium">
          {title}
          <span className="font-mono text-xs text-muted-foreground">{value}</span>
        </CardTitle>
      </CardHeader>
      <CardContent className="py-0">
        {points.length > 1 ? (
          <MetricSparkline points={points} label={title} unit={unit} max={max} />
        ) : (
          <p className="flex h-16 items-center justify-center text-xs text-muted-foreground">
            collecting…
          </p>
        )}
      </CardContent>
    </Card>
  )
}

/** The five host metric cards (CPU / load / memory / disk / network). */
export function HostMetrics({ metrics }: { metrics: MetricsPoint[] | undefined }) {
  const latest = metrics?.at(-1)?.sample
  return (
    <div className="grid grid-cols-2 gap-3 lg:grid-cols-5">
      <MetricCard
        title="CPU"
        value={latest ? `${latest.host.cpu_pct.toFixed(0)}%` : "—"}
        points={series(metrics, (s) => s.host.cpu_pct)}
        unit="%"
        max={100}
      />
      <MetricCard
        title="Load"
        value={
          latest?.host.load_avg_1m != null
            ? formatLoad(latest.host.load_avg_1m, latest.host.cpu_cores)
            : "—"
        }
        points={series(metrics, (s) => s.host.load_avg_1m ?? 0)}
        unit=""
        max={latest?.host.cpu_cores}
      />
      <MetricCard
        title="Memory"
        value={latest ? formatMemGb(latest.host.mem_used_mb, latest.host.mem_total_mb) : "—"}
        points={series(metrics, (s) => s.host.mem_used_mb / 1024)}
        unit="GB"
        max={latest ? latest.host.mem_total_mb / 1024 : undefined}
      />
      <MetricCard
        title="Disk /"
        value={
          latest
            ? `${latest.host.disk_used_gb.toFixed(0)} / ${latest.host.disk_total_gb.toFixed(0)} GB`
            : "—"
        }
        points={series(metrics, (s) => s.host.disk_used_gb)}
        unit="GB"
        max={latest?.host.disk_total_gb}
      />
      <MetricCard
        title="Network"
        value={
          latest
            ? `↓${formatSize(latest.host.net_rx_bps)}/s ↑${formatSize(latest.host.net_tx_bps)}/s`
            : "—"
        }
        points={series(metrics, (s) => (s.host.net_rx_bps + s.host.net_tx_bps) / 1e6)}
        unit="MB/s"
      />
    </div>
  )
}

/** One card per GPU: usage / memory / temperature / power graphs, plus a
 * per-slice memory row when MIG is enabled (NVML attributes no per-slice
 * util, so the full-GPU usage above covers utilization). */
export function GpuTelemetryCard({
  gpu,
  metrics,
}: {
  gpu: GpuSummary
  metrics: MetricsPoint[] | undefined
}) {
  const pick = (f: (g: NonNullable<MetricsPoint["sample"]["gpus"][number]>) => number) =>
    (metrics ?? []).map((p) => ({
      ts: p.sampled_at,
      value: (() => {
        const g = p.sample.gpus.find((x) => x.uuid === gpu.gpu_uuid)
        return g ? f(g) : 0
      })(),
    }))

  const latest = metrics?.at(-1)?.sample.gpus.find((g) => g.uuid === gpu.gpu_uuid)
  const memTotalGb = (gpu.memory_mb ?? 0) / 1024

  const graphs = [
    { key: "usage", title: "Usage", value: latest ? `${latest.util_pct}%` : "—", points: pick((g) => g.util_pct), unit: "%", max: 100 },
    {
      key: "memory",
      title: "Memory",
      value: latest ? `${(latest.mem_used_mb / 1024).toFixed(1)} / ${memTotalGb.toFixed(0)} GB` : "—",
      points: pick((g) => g.mem_used_mb / 1024),
      unit: "GB",
      max: memTotalGb,
    },
    { key: "temperature", title: "Temperature", value: latest ? `${latest.temperature_c}°C` : "—", points: pick((g) => g.temperature_c), unit: "°C", max: 100 },
    { key: "power", title: "Power", value: latest ? `${latest.power_w.toFixed(0)} W` : "—", points: pick((g) => g.power_w), unit: "W", max: undefined },
  ]

  return (
    <Card className="gap-2 py-4">
      <CardHeader className="py-0">
        <CardTitle className="flex items-baseline gap-2 text-sm font-medium">
          GPU {gpu.index} {gpu.model ? `· ${gpu.model}` : ""}
          {gpu.mig_enabled ? <span className="text-xs font-normal text-slot-free">MIG</span> : null}
        </CardTitle>
      </CardHeader>
      <CardContent className="grid grid-cols-2 gap-x-4 gap-y-2 py-0">
        {graphs.map((g) => (
          <div key={g.key}>
            <p className="flex items-baseline justify-between text-xs text-muted-foreground">
              {g.title}
              <span className="font-mono">{g.value}</span>
            </p>
            {g.points.length > 1 ? (
              <MetricSparkline points={g.points} label={g.title} unit={g.unit} max={g.max} />
            ) : (
              <p className="flex h-16 items-center justify-center text-xs text-muted-foreground">
                collecting…
              </p>
            )}
          </div>
        ))}
      </CardContent>
      {gpu.mig_enabled ? <MigSliceMemory gpu={gpu} metrics={metrics} /> : null}
    </Card>
  )
}

/** Per-MIG-slice framebuffer memory, one mini graph per slice, joined to
 * telemetry by the slot's MIG UUID. Renders under the parent GPU card. */
function MigSliceMemory({
  gpu,
  metrics,
}: {
  gpu: GpuSummary
  metrics: MetricsPoint[] | undefined
}) {
  const slices = gpu.slots.filter((s) => s.slot_type === "MIG_SLOT" && s.mig_uuid)
  if (slices.length === 0) return null
  const latest = metrics?.at(-1)?.sample
  return (
    <CardContent className="border-t pt-3">
      <p className="mb-2 text-xs font-medium text-muted-foreground">MIG slice memory</p>
      <div className="grid grid-cols-2 gap-x-4 gap-y-2">
        {slices.map((slot) => {
          const totalGb =
            (latest?.migs.find((m) => m.uuid === slot.mig_uuid)?.mem_total_mb ??
              slot.capacity_mb ??
              0) / 1024
          const usedNow = latest?.migs.find((m) => m.uuid === slot.mig_uuid)?.mem_used_mb
          const points = series(
            metrics,
            (s) => (s.migs.find((m) => m.uuid === slot.mig_uuid)?.mem_used_mb ?? 0) / 1024,
          )
          return (
            <div key={slot.id}>
              <p className="flex items-baseline justify-between text-xs text-muted-foreground">
                <span className="font-mono">SLOT {slot.name}</span>
                <span className="font-mono">
                  {usedNow != null ? `${(usedNow / 1024).toFixed(1)} / ${totalGb.toFixed(0)} GB` : "—"}
                </span>
              </p>
              {points.length > 1 ? (
                <MetricSparkline points={points} label={`SLOT ${slot.name}`} unit="GB" max={totalGb || undefined} />
              ) : (
                <p className="flex h-16 items-center justify-center text-xs text-muted-foreground">
                  collecting…
                </p>
              )}
            </div>
          )
        })}
      </div>
    </CardContent>
  )
}

/** Full telemetry block for one server (host + GPUs + MIG slices), fetching
 * its own 24h series. Used by the fleet-wide Telemetry page. */
export function ServerTelemetry({ server }: { server: ServerSummary }) {
  const metrics = useServerMetrics(server.id)
  return (
    <div className="flex flex-col gap-3">
      <HostMetrics metrics={metrics.data} />
      {server.gpus.length ? (
        <div className="grid gap-3 lg:grid-cols-2">
          {server.gpus.map((gpu) => (
            <GpuTelemetryCard key={gpu.id} gpu={gpu} metrics={metrics.data} />
          ))}
        </div>
      ) : (
        <p className="text-sm text-muted-foreground">No GPUs reported.</p>
      )}
    </div>
  )
}
