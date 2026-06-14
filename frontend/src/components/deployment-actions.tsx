import { RotateCcwIcon, SquareIcon, Trash2Icon, XCircleIcon } from "lucide-react"

import { useConfirm } from "@/components/confirm-context"
import {
  useDismissDeployment,
  useRemoveDeployment,
  useRestartDeployment,
  useStopDeployment,
} from "@/hooks/use-deployments"
import type { DeploymentState } from "@/lib/states"
import { Button } from "@/components/ui/button"

/** Lifecycle action buttons for one deployment (stop · start/re-deploy ·
 * delete · dismiss), state-gated exactly like the Deployments table row.
 * Shared by the table and the deployment detail header so an action taken
 * in either place is reflected everywhere — the mutations invalidate the
 * `["deployments"]` query key, which the list, the detail query, and the
 * slot grid all read. `onRemoved` fires after a terminal action
 * (delete/dismiss) so a single-deployment view can navigate away. */
export function DeploymentActions({
  deployment: d,
  onRemoved,
}: {
  deployment: { id: string; name: string; state: DeploymentState }
  onRemoved?: () => void
}) {
  const confirm = useConfirm()
  const stop = useStopDeployment()
  const restart = useRestartDeployment()
  const remove = useRemoveDeployment()
  const dismiss = useDismissDeployment()
  const busy = stop.isPending || restart.isPending || remove.isPending || dismiss.isPending
  const terminal = onRemoved ? { onSuccess: onRemoved } : undefined

  return (
    <div className="flex shrink-0 gap-1">
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
            title="Start (re-deploys from the stored spec)"
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
                remove.mutate(d.id, terminal)
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
          onClick={() => dismiss.mutate(d.id, terminal)}
          aria-label="Dismiss"
          title="Clear this failed deployment (the slot is already free)"
        >
          <XCircleIcon className="size-3.5" />
        </Button>
      ) : null}
    </div>
  )
}
