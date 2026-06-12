import { ExternalLinkIcon, HardDriveIcon, LockIcon, XCircleIcon } from "lucide-react"

import { useDeploymentDetail, useDismissDeployment, useLatestMetrics } from "@/hooks/use-deployments"
import { formatRelative } from "@/lib/format"
import { DEPLOYMENT_STATE_META } from "@/lib/states"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Separator } from "@/components/ui/separator"
import { Skeleton } from "@/components/ui/skeleton"

/** Slot-chip click-through (docs/UI-DESIGN.md § Slot detail): live
 * state/usage, ports incl. app URLs, mounts, env *names*. Presentational
 * only — lifecycle actions stay on the Deployments page. */
export function SlotDetailDialog({
  deploymentId,
  onClose,
}: {
  deploymentId: string | null
  onClose: () => void
}) {
  const detail = useDeploymentDetail(deploymentId)
  const metrics = useLatestMetrics()
  const dismiss = useDismissDeployment()

  if (!deploymentId) return null
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
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-h-[85svh] overflow-y-auto sm:max-w-lg">
        {detail.isPending || !d ? (
          <div className="space-y-3">
            <Skeleton className="h-6 w-2/3" />
            <Skeleton className="h-24 w-full" />
          </div>
        ) : (
          <>
            <DialogHeader>
              <DialogTitle className="flex items-center gap-2">
                {d.name}
                <span className={`text-sm font-normal ${meta?.textClass ?? ""}`}>
                  {meta?.label}
                </span>
              </DialogTitle>
              <DialogDescription>
                {d.server_name} · {d.gpu_label} / slot {d.slot_name}
              </DialogDescription>
            </DialogHeader>

            {d.status_detail ? (
              <p className="font-mono text-xs text-muted-foreground">{d.status_detail}</p>
            ) : null}
            {d.error_message ? (
              <div className="rounded-md border border-slot-failed/50 bg-slot-failed/10 p-3 text-xs">
                {d.error_message}
              </div>
            ) : null}

            <div className="space-y-1 text-sm">
              <Row label="Image">
                <span className="break-all font-mono text-xs">{d.image_ref}</span>
              </Row>
              {usage ? (
                <Row label="Usage">
                  <span className="tabular-nums">
                    CPU {usage.cpu_pct.toFixed(0)}% · MEM {(usage.mem_used_mb / 1024).toFixed(1)}/
                    {(usage.mem_limit_mb / 1024).toFixed(0)} GB
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

            <Separator />
            <Section title="Ports">
              {d.ports.length === 0 ? (
                <p className="text-xs text-muted-foreground">None published.</p>
              ) : (
                <ul className="space-y-1 font-mono text-xs">
                  {d.ports.map((p) => (
                    <li key={`${p.container_port}/${p.protocol}`}>
                      {p.hostname ? (
                        <a
                          href={`https://${p.hostname}`}
                          target="_blank"
                          rel="noreferrer"
                          className="inline-flex items-center gap-1 text-primary underline-offset-2 hover:underline"
                        >
                          https://{p.hostname}
                          <ExternalLinkIcon className="size-3" aria-hidden />
                        </a>
                      ) : (
                        <span>
                          {p.host_port}→{p.container_port}/{p.protocol}
                        </span>
                      )}
                      <span className="ml-1.5 text-muted-foreground">({p.kind})</span>
                    </li>
                  ))}
                </ul>
              )}
            </Section>

            <Section title="Mounts">
              {d.mounts.length === 0 ? (
                <p className="text-xs text-muted-foreground">No persistent storage attached.</p>
              ) : (
                <ul className="space-y-1.5 text-xs">
                  {d.mounts.map((m) => (
                    <li key={m.container_path} className="flex flex-col">
                      <span className="flex items-center gap-1.5">
                        <HardDriveIcon className="size-3 text-muted-foreground" aria-hidden />
                        <span className="font-medium">
                          {m.volume_name ?? "(volume deleted)"}
                        </span>
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
              )}
            </Section>

            <Section title="Environment">
              {d.env.length === 0 ? (
                <p className="text-xs text-muted-foreground">No variables set.</p>
              ) : (
                <div className="flex flex-wrap gap-1.5">
                  {d.env.map((e) => (
                    <Badge key={e.key} variant="secondary" className="gap-1 font-mono text-[10px]">
                      {e.is_secret ? <LockIcon className="size-2.5" aria-hidden /> : null}
                      {e.key}
                      {e.is_secret ? " = •••" : ""}
                    </Badge>
                  ))}
                </div>
              )}
            </Section>

            {d.state === "FAILED" ? (
              <DialogFooter>
                <Button
                  variant="outline"
                  className="text-slot-failed"
                  disabled={dismiss.isPending}
                  onClick={() =>
                    dismiss.mutate(d.id, { onSuccess: onClose })
                  }
                >
                  <XCircleIcon className="size-3.5" aria-hidden />
                  Clear failed deployment
                </Button>
              </DialogFooter>
            ) : null}
          </>
        )}
      </DialogContent>
    </Dialog>
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
