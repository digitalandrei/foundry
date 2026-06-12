import {
  CircleAlertIcon,
  ExternalLinkIcon,
  RocketIcon,
  RotateCcwIcon,
  SquareIcon,
  Trash2Icon,
} from "lucide-react"

import { EmptyState } from "@/components/empty-state"
import {
  useDeployments,
  useRemoveDeployment,
  useRestartDeployment,
  useStopDeployment,
} from "@/hooks/use-deployments"
import { formatRelative } from "@/lib/format"
import { DEPLOYMENT_STATE_META } from "@/lib/states"
import type { DeploymentSummary } from "@/lib/types"
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
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"

export function DeploymentsPage() {
  const deployments = useDeployments()

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">Deployments</CardTitle>
      </CardHeader>
      <CardContent>
        {deployments.isPending ? (
          <div className="space-y-2">
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
          </div>
        ) : deployments.isError ? (
          <EmptyState icon={RocketIcon} title="Could not load deployments" />
        ) : deployments.data.length === 0 ? (
          <EmptyState
            icon={RocketIcon}
            title="No deployments yet"
            description="Drag a container image onto a free GPU slot on the Dashboard to create the first one."
          />
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Image</TableHead>
                <TableHead>Server</TableHead>
                <TableHead>GPU / Slice</TableHead>
                <TableHead>Ports</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Uptime</TableHead>
                <TableHead>By</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {deployments.data.map((d) => (
                <DeploymentRow key={d.id} deployment={d} />
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  )
}

function DeploymentRow({ deployment: d }: { deployment: DeploymentSummary }) {
  const stop = useStopDeployment()
  const restart = useRestartDeployment()
  const remove = useRemoveDeployment()
  const meta = DEPLOYMENT_STATE_META[d.state]
  const busy = stop.isPending || restart.isPending || remove.isPending

  return (
    <TableRow>
      <TableCell className="font-medium">{d.name}</TableCell>
      <TableCell
        className="max-w-52 truncate font-mono text-xs text-muted-foreground"
        title={d.image_ref}
      >
        {d.image_ref}
      </TableCell>
      <TableCell>{d.server_name}</TableCell>
      <TableCell className="font-mono text-xs">
        {d.gpu_label} / {d.slot_name}
      </TableCell>
      <TableCell className="font-mono text-xs">
        {d.ports.length === 0
          ? "—"
          : d.ports.map((p) =>
              p.hostname ? (
                <a
                  key={`${p.container_port}/${p.protocol}`}
                  href={`https://${p.hostname}`}
                  target="_blank"
                  rel="noreferrer"
                  className="mr-1.5 inline-flex items-center gap-0.5 text-primary underline-offset-2 hover:underline"
                  title={`https://${p.hostname} → :${p.container_port}`}
                >
                  {p.hostname}
                  <ExternalLinkIcon className="size-3" aria-hidden />
                </a>
              ) : (
                <span key={`${p.container_port}/${p.protocol}`} className="mr-1.5">
                  {p.host_port}→{p.container_port}/{p.protocol}
                </span>
              ),
            )}
      </TableCell>
      <TableCell>
        <span className={`flex items-center gap-1.5 ${meta.textClass}`}>
          {meta.label}
          {d.state === "FAILED" && d.error_message ? (
            <Tooltip>
              <TooltipTrigger asChild>
                <CircleAlertIcon className="size-3.5" aria-label="Error detail" />
              </TooltipTrigger>
              <TooltipContent className="max-w-sm">{d.error_message}</TooltipContent>
            </Tooltip>
          ) : null}
        </span>
        {d.status_detail ? (
          <span className="block max-w-52 truncate font-mono text-[10px] text-muted-foreground">
            {d.status_detail}
          </span>
        ) : null}
      </TableCell>
      <TableCell className="text-muted-foreground">
        {d.state === "RUNNING" && d.started_at
          ? formatRelative(d.started_at).replace(" ago", "")
          : "—"}
      </TableCell>
      <TableCell className="text-muted-foreground">{d.created_by_name}</TableCell>
      <TableCell className="text-right">
        <div className="flex justify-end gap-1">
          {d.state === "RUNNING" ? (
            <Button
              variant="outline"
              size="icon"
              className="size-8"
              disabled={busy}
              onClick={() => stop.mutate(d.id)}
              aria-label="Stop"
              title="Stop"
            >
              <SquareIcon className="size-3.5" />
            </Button>
          ) : null}
          {d.state === "STOPPED" || d.state === "FAILED" ? (
            <>
              <Button
                variant="outline"
                size="icon"
                className="size-8"
                disabled={busy}
                onClick={() => restart.mutate(d.id)}
                aria-label="Start"
                title="Start"
              >
                <RotateCcwIcon className="size-3.5" />
              </Button>
              <Button
                variant="outline"
                size="icon"
                className="size-8 text-slot-failed"
                disabled={busy}
                onClick={() => remove.mutate(d.id)}
                aria-label="Remove"
                title="Remove container (persistent volumes survive)"
              >
                <Trash2Icon className="size-3.5" />
              </Button>
            </>
          ) : null}
        </div>
      </TableCell>
    </TableRow>
  )
}
