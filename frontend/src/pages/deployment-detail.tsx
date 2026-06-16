import { useState } from "react"
import { Link, useNavigate, useParams } from "@tanstack/react-router"
import { ArrowLeftIcon, HardDriveIcon, LockIcon } from "lucide-react"

import { ConsolePanel } from "@/components/console-panel"
import { DeploymentActions } from "@/components/deployment-actions"
import { DeploymentPorts } from "@/components/deployment-ports"
import { ShellPanel } from "@/components/shell-panel"
import { useDeploymentDetail, useLatestMetrics } from "@/hooks/use-deployments"
import { formatLoad, formatMemGb, formatRelative } from "@/lib/format"
import { DEPLOYMENT_STATE_META } from "@/lib/states"
import type { DeploymentDetail } from "@/lib/types"
import { cn } from "@/lib/utils"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"

/** Each stacked box fills ~one viewport (minus header h-14 + p-4) on
 * small screens, so an operator swipes details → logs → shell. */
const ONE_SCREEN = "min-h-[calc(100svh-5.5rem)] lg:min-h-0"

type Expanded = "none" | "logs" | "shell"

/** Dedicated full-screen per-deployment page (operator layout): details
 * + ports + mounts on top; the live console and the container shell side
 * by side below (each expandable to full width), stacking one-per-screen
 * on phones. The shell connects only when you click Start. */
export function DeploymentDetailPage() {
  const { deploymentId } = useParams({ from: "/app/deployments/$deploymentId" })
  const navigate = useNavigate()
  const detail = useDeploymentDetail(deploymentId)
  const metrics = useLatestMetrics()
  const [expanded, setExpanded] = useState<Expanded>("none")

  const d = detail.data
  const meta = d ? DEPLOYMENT_STATE_META[d.state] : null
  const usage =
    d?.container_id && metrics.data
      ? metrics.data.servers
          .find((s) => s.server_id === d.server_id)
          ?.sample.containers.find(
            (c) =>
              c.container_id.startsWith(d.container_id ?? "") ||
              (d.container_id ?? "").startsWith(c.container_id),
          )
      : undefined

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center gap-3">
        <Button variant="ghost" size="icon" asChild>
          <Link to="/deployments" aria-label="Back to deployments">
            <ArrowLeftIcon className="size-4" aria-hidden />
          </Link>
        </Button>
        {d ? (
          <>
            <h1 className="text-lg font-semibold">{d.name}</h1>
            <span className={`text-sm ${meta?.textClass ?? ""}`}>{meta?.label}</span>
            <span className="ml-auto min-w-0 truncate text-sm text-muted-foreground">
              {d.server_name} · {d.gpu_label} / slot {d.slot_name}
            </span>
            <DeploymentActions
              deployment={d}
              onRemoved={() => navigate({ to: "/deployments" })}
            />
          </>
        ) : (
          <Skeleton className="h-7 w-64" />
        )}
      </div>

      {detail.isError ? (
        <Card>
          <CardContent className="py-10 text-center text-sm text-muted-foreground">
            This deployment could not be found — it may have been removed.
          </CardContent>
        </Card>
      ) : !d ? (
        <Skeleton className="h-64 w-full" />
      ) : (
        <>
          {/* Top: details + ports + mounts + env */}
          <Card className={cn("flex flex-col", ONE_SCREEN)}>
            <CardHeader className="shrink-0">
              <CardTitle className="text-base">Details</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              {d.status_detail ? (
                <p className="font-mono text-xs text-muted-foreground">{d.status_detail}</p>
              ) : null}
              {d.error_message ? (
                <div className="rounded-md border border-slot-failed/50 bg-slot-failed/10 p-3 text-xs">
                  {d.error_message}
                </div>
              ) : null}

              <div className="grid gap-x-10 gap-y-5 md:grid-cols-2 xl:grid-cols-3">
                <div className="space-y-1 text-sm">
                  <Row label="Image">
                    <span className="break-all font-mono text-xs">{d.image_ref}</span>
                  </Row>
                  {usage ? (
                    <Row label="Usage">
                      <span className="tabular-nums">
                        Load {formatLoad(usage.cpu_pct / 100, usage.cpu_cores)} · MEM{" "}
                        {formatMemGb(usage.mem_used_mb, usage.mem_limit_mb)}
                      </span>
                    </Row>
                  ) : null}
                  <Row label="By">{d.created_by_name}</Row>
                  <Row label="Created">{formatRelative(d.created_at)}</Row>
                  {d.state === "RUNNING" && d.started_at ? (
                    <Row label="Uptime">{formatRelative(d.started_at).replace(" ago", "")}</Row>
                  ) : null}
                  {d.container_id ? (
                    <Row label="Container">
                      <span className="font-mono text-xs">{d.container_id.slice(0, 12)}</span>
                    </Row>
                  ) : null}
                </div>

                <div className="space-y-4">
                  <Section title="Ports">
                    <DeploymentPorts ports={d.ports} className="leading-6" />
                  </Section>
                  <Section title="Mounts">
                    <Mounts d={d} />
                  </Section>
                </div>

                <Section title="Environment">
                  {d.env.length === 0 ? (
                    <p className="text-xs text-muted-foreground">No variables set.</p>
                  ) : (
                    <div className="flex flex-wrap gap-1.5">
                      {d.env.map((e) => (
                        <Badge
                          key={e.key}
                          variant="secondary"
                          className="gap-1 font-mono text-[10px]"
                        >
                          {e.is_secret ? <LockIcon className="size-2.5" aria-hidden /> : null}
                          {e.key}
                          {e.is_secret ? " = •••" : ""}
                        </Badge>
                      ))}
                    </div>
                  )}
                </Section>
              </div>
            </CardContent>
          </Card>

          {/* Bottom: console + shell, side by side on lg (each expandable),
              stacked one-per-screen on phones. */}
          <div className="flex flex-col gap-4 lg:h-[calc(100svh-5.5rem)] lg:flex-row">
            <ConsolePanel
              deploymentId={d.id}
              expanded={expanded === "logs"}
              onToggle={() => setExpanded((e) => (e === "logs" ? "none" : "logs"))}
              className={cn(ONE_SCREEN, expanded === "shell" ? "lg:hidden" : "lg:flex-1")}
            />
            <ShellPanel
              deploymentId={d.id}
              running={d.state === "RUNNING"}
              expanded={expanded === "shell"}
              onToggle={() => setExpanded((e) => (e === "shell" ? "none" : "shell"))}
              className={cn(ONE_SCREEN, expanded === "logs" ? "lg:hidden" : "lg:flex-1")}
            />
          </div>
        </>
      )}
    </div>
  )
}

function Mounts({ d }: { d: DeploymentDetail }) {
  if (d.mounts.length === 0) {
    return <p className="text-xs text-muted-foreground">No persistent storage attached.</p>
  }
  return (
    <ul className="space-y-1.5 text-xs">
      {d.mounts.map((m) => (
        <li key={m.container_path} className="flex flex-col">
          <span className="flex items-center gap-1.5">
            <HardDriveIcon className="size-3 text-muted-foreground" aria-hidden />
            <span className="font-medium">{m.volume_name ?? "(volume deleted)"}</span>
            <span className="font-mono">→ {m.container_path}</span>
            {m.read_only ? (
              <Badge variant="secondary" className="text-[10px]">
                ro
              </Badge>
            ) : null}
          </span>
          <span className="ml-[18px] font-mono text-[10px] text-muted-foreground">
            {m.host_path}
          </span>
        </li>
      ))}
    </ul>
  )
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-baseline gap-2">
      <span className="w-20 shrink-0 text-xs text-muted-foreground">{label}</span>
      <span className="min-w-0">{children}</span>
    </div>
  )
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <p className="mb-1.5 text-sm font-medium">{title}</p>
      {children}
    </div>
  )
}
