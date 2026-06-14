import { useNavigate } from "@tanstack/react-router"
import {
  CircleAlertIcon,
  RocketIcon,
  RotateCcwIcon,
  SquareIcon,
  TerminalIcon,
  Trash2Icon,
  XCircleIcon,
} from "lucide-react"

import { useConfirm } from "@/components/confirm-dialog"
import { DeploymentPorts } from "@/components/deployment-ports"
import { EmptyState } from "@/components/empty-state"
import {
  useDeployments,
  useDismissDeployment,
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
  const navigate = useNavigate()
  const confirm = useConfirm()
  const stop = useStopDeployment()
  const restart = useRestartDeployment()
  const remove = useRemoveDeployment()
  const dismiss = useDismissDeployment()
  const meta = DEPLOYMENT_STATE_META[d.state]
  const busy = stop.isPending || restart.isPending || remove.isPending || dismiss.isPending
  const open = () => navigate({ to: "/deployments/$deploymentId", params: { deploymentId: d.id } })

  return (
    <TableRow
      className="cursor-pointer"
      onClick={open}
      role="button"
      aria-label={`Open ${d.name}`}
    >
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
      <TableCell>
        <DeploymentPorts ports={d.ports} />
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
      {/* Action buttons must not trigger the row's navigate. */}
      <TableCell className="text-right" onClick={(e) => e.stopPropagation()}>
        <div className="flex justify-end gap-1">
          <Button
            variant="outline"
            size="icon"
            className="size-8"
            onClick={open}
            aria-label="Console / logs"
            title="Console & logs"
          >
            <TerminalIcon className="size-3.5" />
          </Button>
          {d.state === "RUNNING" ? (
            <Button
              variant="outline"
              size="icon"
              className="size-8"
              disabled={busy}
              onClick={async () => {
                if (
                  await confirm({
                    title: `Stop "${d.name}"?`,
                    description:
                      "The container is removed; restarting it re-deploys from the stored spec.",
                    destructive: true,
                  })
                )
                  stop.mutate(d.id)
              }}
              aria-label="Stop"
              title="Stop"
            >
              <SquareIcon className="size-3.5" />
            </Button>
          ) : null}
          {d.state === "STOPPED" ? (
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
                onClick={async () => {
                  if (
                    await confirm({
                      title: `Delete "${d.name}"?`,
                      description:
                        "The container and its image are deleted and its captured logs are purged. Persistent volumes survive.",
                      destructive: true,
                    })
                  )
                    remove.mutate(d.id)
                }}
                aria-label="Remove"
                title="Remove container (persistent volumes survive)"
              >
                <Trash2Icon className="size-3.5" />
              </Button>
            </>
          ) : null}
          {d.state === "FAILED" ? (
            <Button
              variant="outline"
              size="icon"
              className="size-8 text-slot-failed"
              disabled={busy}
              onClick={() => dismiss.mutate(d.id)}
              aria-label="Dismiss"
              title="Clear this failed deployment (the slot is already free)"
            >
              <XCircleIcon className="size-3.5" />
            </Button>
          ) : null}
        </div>
      </TableCell>
    </TableRow>
  )
}
