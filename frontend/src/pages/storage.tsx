import { useState } from "react"
import { DatabaseIcon, EraserIcon, Trash2Icon } from "lucide-react"

import { useConfirm } from "@/components/confirm-context"
import { EmptyState } from "@/components/empty-state"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Skeleton } from "@/components/ui/skeleton"
import { Separator } from "@/components/ui/separator"
import { VolumeBrowser } from "@/components/volume-browser"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import {
  useCleanVolume,
  useDeleteVolume,
  useSetVolumeQuota,
  useServerVolumes,
} from "@/hooks/use-deployments"
import { useServers } from "@/hooks/use-servers"
import type { ServerVolume } from "@/lib/types"
import { formatFileSize } from "@/lib/volume-files"
import { Input } from "@/components/ui/input"

export function StoragePage() {
  const servers = useServers()
  const [serverId, setServerId] = useState<string | null>(null)
  const selectedServerId = serverId ?? servers.data?.[0]?.id ?? null
  const volumes = useServerVolumes(selectedServerId)
  const selectedServer = servers.data?.find((server) => server.id === selectedServerId)

  return (
    <Card>
      <CardHeader className="gap-3">
        <CardTitle className="text-base">Persistent Storage</CardTitle>
        <div className="max-w-md">
          <Select value={selectedServerId ?? ""} onValueChange={setServerId}>
            <SelectTrigger aria-label="Server">
              <SelectValue placeholder="Choose a server" />
            </SelectTrigger>
            <SelectContent>
              {(servers.data ?? []).map((server) => (
                <SelectItem key={server.id} value={server.id}>
                  {server.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <p className="text-xs text-muted-foreground">
          Storage is grouped by deploy name, then mount name. Slot volumes follow one physical
          GPU or GPU group; server volumes follow the same deploy name across that server. Clean
          keeps the volume identity; delete removes it.
        </p>
      </CardHeader>
      <CardContent>
        {servers.isPending || volumes.isPending ? (
          <div className="space-y-2">
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
          </div>
        ) : servers.isError || volumes.isError ? (
          <EmptyState icon={DatabaseIcon} title="Could not load persistent storage" />
        ) : !selectedServerId ? (
          <EmptyState
            icon={DatabaseIcon}
            title="Choose a server"
            description="Persistent storage is local to its physical slot or server."
          />
        ) : volumes.data.length === 0 ? (
          <EmptyState
            icon={DatabaseIcon}
            title="No volumes in this scope"
            description="Deploy an image with a persistent mount to create the first one."
          />
        ) : (
          <div className="space-y-6">
            {selectedServer ? (
              <VolumeBrowser server={selectedServer} volumes={volumes.data} />
            ) : null}
            <Separator />
            <div>
              <h2 className="mb-3 text-sm font-medium">Volume Policies</h2>
              <VolumeTable volumes={volumes.data} />
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  )
}

function VolumeTable({ volumes }: { volumes: ServerVolume[] }) {
  const clean = useCleanVolume()
  const remove = useDeleteVolume()
  const confirm = useConfirm()

  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Deploy name</TableHead>
          <TableHead>Name</TableHead>
          <TableHead>Placement</TableHead>
          <TableHead>Creator</TableHead>
          <TableHead>Usage / quota</TableHead>
          <TableHead>Attached to</TableHead>
          <TableHead className="text-right">Actions</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {volumes.map((volume) => {
          const attached = volume.attached_to.length > 0
          return (
            <TableRow key={volume.id}>
              <TableCell className="font-mono text-xs">{volume.project_name}</TableCell>
              <TableCell className="font-medium">{volume.name}</TableCell>
              <TableCell>
                {volume.placement === "SERVER"
                  ? "Server"
                  : volume.gpu_group_id
                    ? `Group ${volume.group_name ?? "unknown"}`
                    : `Slot ${volume.slot_name ?? "unknown"}`}
              </TableCell>
              <TableCell>{volume.created_by_name}</TableCell>
              <TableCell><QuotaCell volume={volume} /></TableCell>
              <TableCell className="text-muted-foreground">
                {attached ? volume.attached_to.join(", ") : "Not attached"}
              </TableCell>
              <TableCell className="text-right">
                <div className="flex justify-end gap-1.5">
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={!volume.can_manage || attached || clean.isPending}
                    onClick={async () => {
                      const accepted = await confirm({
                        title: `Clean ${volume.name}?`,
                        description:
                          "This irreversibly removes every file but keeps the volume available for reuse.",
                        confirmLabel: "CLEAN VOLUME",
                        destructive: true,
                        requireConfirmText: volume.name,
                      })
                      if (accepted) clean.mutate(volume.id)
                    }}
                  >
                    <EraserIcon className="size-3.5" aria-hidden />
                    Clean
                  </Button>
                  <Button
                    variant="outline"
                    size="icon-sm"
                    aria-label={`Delete volume ${volume.name}`}
                    disabled={!volume.can_manage || attached || remove.isPending}
                    onClick={async () => {
                      const accepted = await confirm({
                        title: `Delete ${volume.name}?`,
                        description:
                          "This irreversibly removes the volume identity and all of its files.",
                        confirmLabel: "DELETE VOLUME",
                        destructive: true,
                        requireConfirmText: volume.name,
                      })
                      if (accepted) remove.mutate(volume.id)
                    }}
                  >
                    <Trash2Icon className="size-3.5" aria-hidden />
                  </Button>
                </div>
              </TableCell>
            </TableRow>
          )
        })}
      </TableBody>
    </Table>
  )
}

function QuotaCell({ volume }: { volume: ServerVolume }) {
  const setQuota = useSetVolumeQuota()
  const initial = volume.quota_bytes == null ? "" : (volume.quota_bytes / 1024 ** 3).toFixed(1)
  const [value, setValue] = useState(initial)
  return (
    <div className="min-w-48 space-y-1">
      <p className="text-xs text-muted-foreground">
        {volume.used_bytes == null ? "Measuring…" : formatFileSize(volume.used_bytes)} / {volume.quota_bytes == null ? "unlimited" : formatFileSize(volume.quota_bytes)}
      </p>
      {volume.can_manage ? (
        <div className="flex items-center gap-1">
          <Input className="h-7 w-20 text-xs" type="number" min="0.001" step="0.5" value={value} onChange={(event) => setValue(event.target.value)} placeholder="GiB" aria-label={`Quota for ${volume.name} in GiB`} />
          <Button variant="outline" size="sm" className="h-7" disabled={setQuota.isPending || !value} onClick={() => setQuota.mutate({ id: volume.id, quotaBytes: Math.round(Number(value) * 1024 ** 3) })}>Set</Button>
          <Button variant="ghost" size="sm" className="h-7" disabled={setQuota.isPending || volume.quota_bytes == null} onClick={() => { setValue(""); setQuota.mutate({ id: volume.id, quotaBytes: null }) }}>∞</Button>
        </div>
      ) : null}
    </div>
  )
}
