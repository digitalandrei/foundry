import { Link, useParams } from "@tanstack/react-router"
import { ArrowLeftIcon } from "lucide-react"

import { MetricSparkline } from "@/components/metric-sparkline"
import { useServerDetail, useServerMetrics } from "@/hooks/use-servers"
import { formatLoad, formatMemGb, formatSize } from "@/lib/format"
import { SERVER_STATUS_META } from "@/lib/states"
import type { GpuSummary, MetricsPoint, ServerContainer } from "@/lib/types"
import { cn } from "@/lib/utils"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"

/** Dedicated per-server page (operator requirement): host metrics,
 * GPU utilization, containers with ports. Data: detail 15s, metrics
 * series 30s/24h. */
export function ServerDetailPage() {
  const { serverId } = useParams({ from: "/app/servers/$serverId" })
  const detail = useServerDetail(serverId)
  const metrics = useServerMetrics(serverId)

  const latest = metrics.data?.at(-1)?.sample

  return (
    <div className="mx-auto flex max-w-5xl flex-col gap-4">
      <div className="flex items-center gap-3">
        <Button variant="ghost" size="icon" asChild>
          <Link to="/servers" aria-label="Back to servers">
            <ArrowLeftIcon className="size-4" aria-hidden />
          </Link>
        </Button>
        {detail.data ? (
          <>
            <span
              className={cn(
                "size-2.5 rounded-full",
                SERVER_STATUS_META[detail.data.server.status].dotClass,
              )}
              aria-hidden
            />
            <h1 className="text-lg font-semibold">{detail.data.server.name}</h1>
            <span className="text-sm text-muted-foreground">{detail.data.server.hostname}</span>
            <span className="ml-auto flex gap-3 text-xs text-muted-foreground">
              <span>{detail.data.server.os_version}</span>
              <span>Docker {detail.data.docker_version ?? "—"}</span>
              <span>Driver {detail.data.nvidia_driver_version ?? "—"}</span>
              <span>Agent v{detail.data.server.agent_version ?? "?"}</span>
            </span>
          </>
        ) : (
          <Skeleton className="h-7 w-64" />
        )}
      </div>

      {/* Host metrics */}
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-5">
        <MetricCard
          title="CPU"
          value={latest ? `${latest.host.cpu_pct.toFixed(0)}%` : "—"}
          points={series(metrics.data, (s) => s.host.cpu_pct)}
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
          points={series(metrics.data, (s) => s.host.load_avg_1m ?? 0)}
          unit=""
          max={latest?.host.cpu_cores}
        />
        <MetricCard
          title="Memory"
          value={latest ? formatMemGb(latest.host.mem_used_mb, latest.host.mem_total_mb) : "—"}
          points={series(metrics.data, (s) => s.host.mem_used_mb / 1024)}
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
          points={series(metrics.data, (s) => s.host.disk_used_gb)}
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
          points={series(metrics.data, (s) => (s.host.net_rx_bps + s.host.net_tx_bps) / 1e6)}
          unit="MB/s"
        />
      </div>

      {/* GPUs */}
      {detail.data?.gpus.length ? (
        <div className="grid gap-3 lg:grid-cols-2">
          {detail.data.gpus.map((gpu) => (
            <GpuCard key={gpu.id} gpu={gpu} metrics={metrics.data} />
          ))}
        </div>
      ) : null}

      {/* Containers */}
      <Card>
        <CardHeader>
          <CardTitle className="text-base">
            Containers ({detail.data?.containers.length ?? "…"})
          </CardTitle>
        </CardHeader>
        <CardContent>
          {detail.data ? (
            <ContainersTable containers={detail.data.containers} latest={latest} />
          ) : (
            <Skeleton className="h-24 w-full" />
          )}
        </CardContent>
      </Card>
    </div>
  )
}

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

/** One card per GPU, with grouped graphs: usage, memory, temperature,
 * power (operator requirement). Percent/capacity series are pinned to
 * their full scale; power auto-scales (no limit reported yet). */
function GpuCard({ gpu, metrics }: { gpu: GpuSummary; metrics: MetricsPoint[] | undefined }) {
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
    {
      key: "usage",
      title: "Usage",
      value: latest ? `${latest.util_pct}%` : "—",
      points: pick((g) => g.util_pct),
      unit: "%",
      max: 100,
    },
    {
      key: "memory",
      title: "Memory",
      value: latest
        ? `${(latest.mem_used_mb / 1024).toFixed(1)} / ${memTotalGb.toFixed(0)} GB`
        : "—",
      points: pick((g) => g.mem_used_mb / 1024),
      unit: "GB",
      max: memTotalGb,
    },
    {
      key: "temperature",
      title: "Temperature",
      value: latest ? `${latest.temperature_c}°C` : "—",
      points: pick((g) => g.temperature_c),
      unit: "°C",
      max: 100,
    },
    {
      key: "power",
      title: "Power",
      value: latest ? `${latest.power_w.toFixed(0)} W` : "—",
      points: pick((g) => g.power_w),
      unit: "W",
      max: undefined,
    },
  ]

  return (
    <Card className="gap-2 py-4">
      <CardHeader className="py-0">
        <CardTitle className="text-sm font-medium">
          GPU {gpu.index} {gpu.model ? `· ${gpu.model}` : ""}
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
    </Card>
  )
}

function ContainersTable({
  containers,
  latest,
}: {
  containers: ServerContainer[]
  latest: MetricsPoint["sample"] | undefined
}) {
  if (containers.length === 0) {
    return <p className="text-sm text-muted-foreground">No containers reported.</p>
  }
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Name</TableHead>
          <TableHead>Image</TableHead>
          <TableHead>State</TableHead>
          <TableHead>Load</TableHead>
          <TableHead>Memory</TableHead>
          <TableHead>Ports</TableHead>
          <TableHead />
        </TableRow>
      </TableHeader>
      <TableBody>
        {containers.map((c) => {
          const m = latest?.containers.find((x) => x.container_id === c.container_id)
          return (
            <TableRow key={c.container_id}>
              <TableCell className="font-medium">{c.name}</TableCell>
              <TableCell
                className="max-w-44 truncate font-mono text-xs text-muted-foreground"
                title={c.image}
              >
                {c.image}
              </TableCell>
              <TableCell
                className={c.state === "running" ? "text-slot-free" : "text-muted-foreground"}
                title={c.status}
              >
                {c.state}
              </TableCell>
              <TableCell className="font-mono text-xs">
                {m ? formatLoad(m.cpu_pct / 100, m.cpu_cores) : "—"}
              </TableCell>
              <TableCell className="font-mono text-xs">
                {m ? formatMemGb(m.mem_used_mb, m.mem_limit_mb) : "—"}
              </TableCell>
              <TableCell className="font-mono text-xs">
                {c.ports.length === 0
                  ? "—"
                  : c.ports.map((p) => (
                      <span key={`${p.container_port}/${p.protocol}/${p.host_port}`} className="mr-1.5">
                        {p.host_port != null
                          ? `${p.host_port}→${p.container_port}`
                          : p.container_port}
                        /{p.protocol}
                      </span>
                    ))}
              </TableCell>
              <TableCell>
                {c.managed ? <Badge variant="secondary">foundry</Badge> : null}
              </TableCell>
            </TableRow>
          )
        })}
      </TableBody>
    </Table>
  )
}
