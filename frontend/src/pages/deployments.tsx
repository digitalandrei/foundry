import { Link } from "@tanstack/react-router"
import { CircleAlertIcon, RocketIcon, TerminalIcon } from "lucide-react"

import { DeploymentActions } from "@/components/deployment-actions"
import { DeploymentPorts } from "@/components/deployment-ports"
import { EmptyState } from "@/components/empty-state"
import { useDeployments } from "@/hooks/use-deployments"
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
  const meta = DEPLOYMENT_STATE_META[d.state]

  return (
    <TableRow>
      <TableCell className="font-medium">
        <Link
          to="/deployments/$deploymentId"
          params={{ deploymentId: d.id }}
          className="rounded-sm underline-offset-4 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          {d.name}
        </Link>
        {d.adopted ? (
          <span className="ml-2 rounded bg-muted px-1.5 py-0.5 text-[10px] font-normal text-muted-foreground align-middle">
            adopted
          </span>
        ) : null}
      </TableCell>
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
      <TableCell className="text-right">
        <div className="flex justify-end gap-1">
          <Button
            variant="outline"
            size="icon"
            className="size-8"
            aria-label="Console / logs"
            title="Console & logs"
            asChild
          >
            <Link
              to="/deployments/$deploymentId"
              params={{ deploymentId: d.id }}
            >
              <TerminalIcon className="size-3.5" />
            </Link>
          </Button>
          <DeploymentActions deployment={d} />
        </div>
      </TableCell>
    </TableRow>
  )
}
