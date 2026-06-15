import { ServerOffIcon } from "lucide-react"

import { EmptyState } from "@/components/empty-state"
import { useDeployments } from "@/hooks/use-deployments"
import { useServers } from "@/hooks/use-servers"
import { formatSize } from "@/lib/format"
import { occupantsBySlot, slotDeployability } from "@/lib/slots"
import { SERVER_STATUS_META, SLOT_STATE_META } from "@/lib/states"
import type { DeploymentSummary, DragTagData, DropSlotData, ServerSummary, SlotSummary } from "@/lib/types"
import { cn } from "@/lib/utils"
import { Badge } from "@/components/ui/badge"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Skeleton } from "@/components/ui/skeleton"

/** Tap-to-deploy destination picker — the touch/keyboard counterpart to
 * dragging a tag onto a slot (docs/UI-DESIGN.md § Deploying). Lists every
 * server's GPUs and slots; tapping a free slot deploys, a running one
 * replaces. Selection hands a (tag, slot) pair back to the dashboard,
 * which opens the very same DeployDialog as the drag path. */
export function SlotPickerDialog({
  tag,
  onClose,
  onSelect,
}: {
  tag: DragTagData | null
  onClose: () => void
  onSelect: (tag: DragTagData, slot: DropSlotData) => void
}) {
  const servers = useServers()
  const deployments = useDeployments()

  if (!tag) return null
  const occupants = occupantsBySlot(deployments.data)

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-h-[85svh] overflow-y-auto sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <span className="truncate">Deploy {tag.imageName}</span>
            <Badge variant="secondary" className="shrink-0 font-mono text-[11px]">
              {tag.tagName}
            </Badge>
          </DialogTitle>
          <DialogDescription>
            Pick a GPU slot — a free slot deploys here, a running slot replaces what&apos;s on it.
            {tag.sizeBytes != null ? ` · ${formatSize(tag.sizeBytes)}` : ""}
          </DialogDescription>
        </DialogHeader>

        {servers.isPending ? (
          <div className="space-y-2">
            <Skeleton className="h-16 w-full" />
            <Skeleton className="h-16 w-full" />
          </div>
        ) : servers.isError ? (
          <EmptyState icon={ServerOffIcon} title="Could not load servers" />
        ) : servers.data.length === 0 ? (
          <EmptyState
            icon={ServerOffIcon}
            title="No servers enrolled"
            description="Enroll a GPU server to deploy onto its slots."
          />
        ) : (
          <div className="flex flex-col gap-4">
            {servers.data.map((server) => (
              <ServerSlots
                key={server.id}
                server={server}
                occupants={occupants}
                onPick={(slot) =>
                  onSelect(tag, {
                    slotId: slot.id,
                    slotName: slot.name,
                    slotState: slot.state,
                    serverId: server.id,
                    serverName: server.name,
                  })
                }
              />
            ))}
          </div>
        )}
      </DialogContent>
    </Dialog>
  )
}

function ServerSlots({
  server,
  occupants,
  onPick,
}: {
  server: ServerSummary
  occupants: Map<string, DeploymentSummary>
  onPick: (slot: SlotSummary) => void
}) {
  const meta = SERVER_STATUS_META[server.status]
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <span className={cn("size-2.5 shrink-0 rounded-full", meta.dotClass)} aria-hidden />
        <span className="font-medium">{server.name}</span>
        {server.status !== "ONLINE" ? (
          <span className={cn("text-xs", meta.textClass)}>{meta.label}</span>
        ) : server.docker_ok === false ? (
          <span className="text-xs text-slot-failed">Docker stopped</span>
        ) : null}
      </div>
      {server.gpus.length === 0 ? (
        <p className="pl-4 text-xs text-muted-foreground">No GPUs reported yet.</p>
      ) : (
        server.gpus.map((gpu) => (
          <div key={gpu.id} className="pl-4">
            <p className="mb-1 text-xs font-medium text-muted-foreground">
              GPU {gpu.index}
              {gpu.model ? ` · ${gpu.model}` : ""}
            </p>
            <div className="grid gap-1.5 [grid-template-columns:repeat(auto-fill,minmax(9.5rem,1fr))]">
              {gpu.slots.map((slot) => (
                <SlotOption
                  key={slot.id}
                  slot={slot}
                  server={server}
                  occupant={occupants.get(slot.id)}
                  onPick={() => onPick(slot)}
                />
              ))}
            </div>
          </div>
        ))
      )}
    </div>
  )
}

function SlotOption({
  slot,
  server,
  occupant,
  onPick,
}: {
  slot: SlotSummary
  server: ServerSummary
  occupant: DeploymentSummary | undefined
  onPick: () => void
}) {
  const meta = SLOT_STATE_META[slot.state]
  const { deployable, replace, reason } = slotDeployability(slot, server, occupant)
  const capacity =
    slot.mig_profile ?? (slot.capacity_mb != null ? `${Math.round(slot.capacity_mb / 1024)} GB` : "")

  return (
    <button
      type="button"
      disabled={!deployable}
      onClick={onPick}
      title={reason ?? undefined}
      className={cn(
        "flex flex-col items-start gap-0.5 rounded-md border px-2.5 py-2 text-left transition-colors",
        deployable
          ? "cursor-pointer hover:border-slot-running hover:bg-accent/50"
          : "cursor-not-allowed opacity-50",
        replace && "border-slot-running/50",
      )}
    >
      <span className="flex w-full items-center justify-between gap-2">
        <span className="font-mono text-xs font-medium">SLOT {slot.name}</span>
        <span className={cn("shrink-0 text-[10px]", meta.textClass)}>{meta.label}</span>
      </span>
      <span className="flex w-full items-center justify-between gap-2 text-[11px] text-muted-foreground">
        <span className="truncate">
          {deployable ? (replace ? `Replace ${occupant?.name ?? "running"}` : "Deploy here") : reason}
        </span>
        {capacity ? <span className="shrink-0">{capacity}</span> : null}
      </span>
    </button>
  )
}
