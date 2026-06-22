import { BoxesIcon, ServerOffIcon } from "lucide-react"

import { EmptyState } from "@/components/empty-state"
import { useDeployments } from "@/hooks/use-deployments"
import { useServerGroups } from "@/hooks/use-gpu-groups"
import { useServers } from "@/hooks/use-servers"
import { formatSize } from "@/lib/format"
import { gpuSlotPositions, occupantsBySlot, slotDeployability, type SlotPosition } from "@/lib/slots"
import { SERVER_STATUS_META } from "@/lib/states"
import type {
  DeploymentSummary,
  DragTagData,
  DropData,
  GpuGroup,
  ServerSummary,
} from "@/lib/types"
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
  onSelect: (tag: DragTagData, drop: DropData) => void
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
                onPickSlot={(position, replace) =>
                  onSelect(tag, {
                    kind: "slot",
                    slotId: position.slot.id,
                    slotName: position.label,
                    slotState: position.slot.state,
                    serverId: server.id,
                    serverName: server.name,
                    replace,
                  })
                }
                onPickGroup={(group) =>
                  onSelect(tag, {
                    kind: "group",
                    groupId: group.id,
                    groupName: group.name,
                    serverId: server.id,
                    serverName: server.name,
                    memberCount: group.gpu_ids.length,
                    vramMb: group.combined_vram_mb,
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
  onPickSlot,
  onPickGroup,
}: {
  server: ServerSummary
  occupants: Map<string, DeploymentSummary[]>
  onPickSlot: (position: SlotPosition, replace: boolean) => void
  onPickGroup: (group: GpuGroup) => void
}) {
  const meta = SERVER_STATUS_META[server.status]
  const groups = useServerGroups(server.id)
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
              {gpuSlotPositions(gpu, occupants).map((position) => (
                <SlotOption
                  key={`${position.slot.id}#${position.label}`}
                  position={position}
                  server={server}
                  onPick={(replace) => onPickSlot(position, replace)}
                />
              ))}
            </div>
          </div>
        ))
      )}
      {groups.data && groups.data.length > 0 ? (
        <div className="pl-4">
          <p className="mb-1 flex items-center gap-1 text-xs font-medium text-muted-foreground">
            <BoxesIcon className="size-3.5" aria-hidden /> Groups
          </p>
          <div className="grid gap-1.5 [grid-template-columns:repeat(auto-fill,minmax(9.5rem,1fr))]">
            {groups.data.map((group) => (
              <GroupOption key={group.id} group={group} onPick={() => onPickGroup(group)} />
            ))}
          </div>
        </div>
      ) : null}
    </div>
  )
}

/** Tap target for a group: deploy across all member GPUs. Backend-computed
 * `deployable`/`busy_reason` drive enablement (never recomputed here). */
function GroupOption({ group, onPick }: { group: GpuGroup; onPick: () => void }) {
  const vramGb = Math.round(group.combined_vram_mb / 1024)
  return (
    <button
      type="button"
      disabled={!group.deployable}
      onClick={onPick}
      title={group.deployable ? undefined : (group.busy_reason ?? undefined)}
      className={cn(
        "flex flex-col items-start gap-0.5 rounded-md border px-2.5 py-2 text-left transition-colors",
        group.deployable
          ? "cursor-pointer hover:border-slot-running hover:bg-accent/50"
          : "cursor-not-allowed opacity-50",
      )}
    >
      <span className="font-mono text-xs font-medium">{group.name}</span>
      <span className="flex w-full items-center justify-between gap-2 text-[11px] text-muted-foreground">
        <span className="truncate">{group.deployable ? "Deploy here" : group.busy_reason}</span>
        <span className="shrink-0">
          {group.gpu_ids.length} GPU · {vramGb} GB
        </span>
      </span>
    </button>
  )
}

/** One deploy position ("SLOT n"): a free position deploys, an occupied
 * single-use slot replaces, any other occupant is shown but disabled. */
function SlotOption({
  position,
  server,
  onPick,
}: {
  position: SlotPosition
  server: ServerSummary
  onPick: (replace: boolean) => void
}) {
  const { slot, label, occupant, occupants } = position
  const { deployable, replace, reason } = slotDeployability(slot, server, occupants)
  const isReplace = occupant !== undefined && replace && slot.max_occupants === 1
  const enabled = occupant === undefined ? deployable : isReplace

  const action = !enabled
    ? occupant
      ? `In use · ${occupant.name}`
      : (reason ?? "unavailable")
    : isReplace
      ? `Replace ${occupant?.name ?? "running"}`
      : "Deploy here"

  return (
    <button
      type="button"
      disabled={!enabled}
      onClick={() => onPick(isReplace)}
      title={!enabled ? (occupant?.image_ref ?? reason ?? undefined) : undefined}
      className={cn(
        "flex flex-col items-start gap-0.5 rounded-md border px-2.5 py-2 text-left transition-colors",
        enabled
          ? "cursor-pointer hover:border-slot-running hover:bg-accent/50"
          : "cursor-not-allowed opacity-50",
        isReplace && "border-slot-running/50",
      )}
    >
      <span className="flex w-full items-center justify-between gap-2">
        <span className="font-mono text-xs font-medium">SLOT {label}</span>
        <span className={cn("shrink-0 text-[10px]", occupant ? "text-muted-foreground" : "text-slot-free")}>
          {occupant
            ? occupant.state === "RUNNING"
              ? "running"
              : occupant.state.toLowerCase().replace(/_/g, " ")
            : "Free"}
        </span>
      </span>
      <span className="flex w-full items-center justify-between gap-2 text-[11px] text-muted-foreground">
        <span className="truncate">{action}</span>
      </span>
    </button>
  )
}
