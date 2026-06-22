import { Link, useParams } from "@tanstack/react-router"
import { ArrowLeftIcon } from "lucide-react"

import { GpuTelemetryCard, HostMetrics } from "@/components/server-telemetry"
import { ServerGpuConfig } from "@/components/server-gpu-config"
import { useMe } from "@/hooks/use-auth"
import { useAdoptContainer } from "@/hooks/use-deployments"
import { useServerDetail, useServerMetrics } from "@/hooks/use-servers"
import { formatLoad, formatMemGb } from "@/lib/format"
import { SERVER_STATUS_META } from "@/lib/states"
import type { MetricsPoint, ServerContainer } from "@/lib/types"
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
  const me = useMe()

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
      <HostMetrics metrics={metrics.data} />

      {/* GPUs (incl. per-MIG-slice memory) */}
      {detail.data?.gpus.length ? (
        <div className="grid gap-3 lg:grid-cols-2">
          {detail.data.gpus.map((gpu) => (
            <GpuTelemetryCard key={gpu.id} gpu={gpu} metrics={metrics.data} />
          ))}
        </div>
      ) : null}

      {/* GPU groups & slot-sharing config — admin-only. */}
      {detail.data && me.data?.is_admin ? <ServerGpuConfig server={detail.data.server} /> : null}

      {/* Containers */}
      <Card>
        <CardHeader>
          <CardTitle className="text-base">
            Containers ({detail.data?.containers.length ?? "…"})
          </CardTitle>
        </CardHeader>
        <CardContent>
          {detail.data ? (
            <ContainersTable
              containers={detail.data.containers}
              latest={latest}
              serverId={detail.data.server.id}
              isAdmin={me.data?.is_admin ?? false}
            />
          ) : (
            <Skeleton className="h-24 w-full" />
          )}
        </CardContent>
      </Card>
    </div>
  )
}

function ContainersTable({
  containers,
  latest,
  serverId,
  isAdmin,
}: {
  containers: ServerContainer[]
  latest: MetricsPoint["sample"] | undefined
  serverId: string
  isAdmin: boolean
}) {
  const adopt = useAdoptContainer()
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
          <TableHead>Mounts</TableHead>
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
              <TableCell className="max-w-56 font-mono text-xs text-muted-foreground">
                {c.mounts.length === 0 ? (
                  "—"
                ) : (
                  <div className="space-y-0.5">
                    {c.mounts.map((mt) => (
                      <div
                        key={`${mt.source}:${mt.destination}`}
                        className="truncate"
                        title={`${mt.source || mt.mount_type} → ${mt.destination}${mt.read_only ? " (ro)" : ""}`}
                      >
                        {mt.source || mt.mount_type}→{mt.destination}
                        {mt.read_only ? " (ro)" : ""}
                      </div>
                    ))}
                  </div>
                )}
              </TableCell>
              <TableCell className="text-right">
                {c.managed ? (
                  <Badge variant="secondary">foundry</Badge>
                ) : isAdmin ? (
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-7"
                    disabled={adopt.isPending}
                    onClick={() => adopt.mutate({ serverId, containerId: c.container_id })}
                    title="Adopt this container so Foundry can control it (logs, console, stop, delete)"
                  >
                    Adopt
                  </Button>
                ) : null}
              </TableCell>
            </TableRow>
          )
        })}
      </TableBody>
    </Table>
  )
}
