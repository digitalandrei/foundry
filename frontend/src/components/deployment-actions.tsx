import { RefreshCwIcon, RotateCcwIcon, SquareIcon, Trash2Icon, XCircleIcon } from "lucide-react"

import { useConfirm } from "@/components/confirm-context"
import {
  useDismissDeployment,
  useRemoveDeployment,
  useRestartDeployment,
  useRetryPublish,
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
  deployment: { id: string; name: string; state: DeploymentState; adopted?: boolean }
  onRemoved?: () => void
}) {
  const confirm = useConfirm()
  // Adopted (externally-created) containers get a double-confirmation:
  // Foundry did not create them, so stop/delete is type-to-confirm.
  const adoptedNote = d.adopted
    ? " This is an adopted container Foundry did not create — stopping it affects whatever started it."
    : ""
  const requireConfirmText = d.adopted ? d.name : undefined
  const stop = useStopDeployment()
  const restart = useRestartDeployment()
  const remove = useRemoveDeployment()
  const dismiss = useDismissDeployment()
  const retryPublish = useRetryPublish()
  const busy = stop.isPending || restart.isPending || remove.isPending || dismiss.isPending || retryPublish.isPending
  const terminal = onRemoved ? { onSuccess: onRemoved } : undefined

  return (
    <div className="flex shrink-0 gap-1">
      {d.state === "RUNNING" || d.state === "PUBLISH_FAILED" ? (
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
                  "The container is removed; restarting it re-deploys from the stored spec." +
                  adoptedNote,
                destructive: true,
                requireConfirmText,
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
      {d.state === "PUBLISH_FAILED" ? (
        <>
          <Button
            variant="outline"
            size="sm"
            className="h-8"
            disabled={busy}
            onClick={() => retryPublish.mutate(d.id)}
            title="Retry only the nginx vhost; the healthy container is retained"
          >
            <RefreshCwIcon className="size-3.5" />
            Retry publishing
          </Button>
          <Button
            variant="outline"
            size="icon"
            className="size-8 text-slot-failed"
            disabled={busy}
            onClick={async () => {
              if (await confirm({
                title: `Delete "${d.name}"?`,
                description: "The retained container is removed. Persistent volumes survive.",
                destructive: true,
              })) remove.mutate(d.id, terminal)
            }}
            aria-label="Delete"
            title="Delete retained container"
          >
            <Trash2Icon className="size-3.5" />
          </Button>
        </>
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
                    "The container and its image are deleted and its captured logs are purged. Persistent volumes survive." +
                    adoptedNote,
                  destructive: true,
                  requireConfirmText,
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
