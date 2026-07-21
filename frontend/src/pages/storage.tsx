import { useEffect, useMemo, useRef, useState } from "react"
import { DatabaseIcon, EraserIcon, SearchIcon, Trash2Icon } from "lucide-react"

import { useConfirm } from "@/components/confirm-context"
import { EmptyState } from "@/components/empty-state"
import { ServerPicker } from "@/components/server-picker"
import { VolumeLocationLabel } from "@/components/volume-location"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
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
import { searchablePickerMatches } from "@/lib/searchable-picker"
import type { ServerSummary, ServerVolume } from "@/lib/types"
import { formatFileSize } from "@/lib/volume-files"
import { volumeLocation } from "@/lib/volume-locations"
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
          <ServerPicker
            value={selectedServerId ?? ""}
            servers={servers.data ?? []}
            onValueChange={setServerId}
          />
        </div>
        <p className="text-xs text-muted-foreground">
          Storage is grouped by deploy name, then mount name. Slot volumes follow one physical
          GPU or GPU group; server volumes follow the same deploy name across that server. Clean
          keeps the volume identity; delete removes it.
        </p>
      </CardHeader>
      <CardContent>
        {servers.isPending ? (
          <div className="space-y-2">
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
          </div>
        ) : servers.isError ? (
          <EmptyState icon={DatabaseIcon} title="Could not load persistent storage" />
        ) : !selectedServerId || !selectedServer ? (
          <EmptyState
            icon={DatabaseIcon}
            title="Choose a server"
            description="Persistent storage is local to its physical slot or server."
          />
        ) : volumes.isPending ? (
          <div className="space-y-2">
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
          </div>
        ) : volumes.isError ? (
          <EmptyState icon={DatabaseIcon} title="Could not load persistent storage" />
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
              <VolumeTable server={selectedServer} volumes={volumes.data} />
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  )
}

function VolumeTable({
  server,
  volumes,
}: {
  server: Pick<ServerSummary, "name" | "hostname">
  volumes: ServerVolume[]
}) {
  const clean = useCleanVolume()
  const remove = useDeleteVolume()
  const confirm = useConfirm()
  const [query, setQuery] = useState("")
  const filtered = useMemo(
    () =>
      volumes.filter((volume) => {
        const location = volumeLocation(volume, server)
        return searchablePickerMatches(
          { label: location.breadcrumb, searchText: location.searchText },
          query,
        )
      }),
    [query, server, volumes],
  )

  return (
    <div className="space-y-3">
      <div className="relative max-w-md">
        <SearchIcon
          className="pointer-events-none absolute top-1/2 left-3 size-3.5 -translate-y-1/2 text-muted-foreground"
          aria-hidden
        />
        <Input
          type="search"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Search node, placement, deploy name, mount, or attachment…"
          aria-label="Search volume policies"
          className="pl-8"
        />
      </div>
      {filtered.length === 0 ? (
        <EmptyState
          icon={SearchIcon}
          title="No matching volume policies"
          description="Search node, hostname, placement, deploy name, mount name, or attached deployment."
        />
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Storage namespace</TableHead>
              <TableHead>Creator</TableHead>
              <TableHead>Usage / quota</TableHead>
              <TableHead>Attached to</TableHead>
              <TableHead className="text-right">Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {filtered.map((volume) => {
              const attached = volume.attached_to.length > 0
              const cleanReason = volumeActionDisabledReason(
                volume,
                attached,
                clean.isPending,
                "clean",
              )
              const deleteReason = volumeActionDisabledReason(
                volume,
                attached,
                remove.isPending,
                "delete",
              )
              const cleanDescriptionId = `volume-${volume.id}-clean-reason`
              const deleteDescriptionId = `volume-${volume.id}-delete-reason`
              return (
                <TableRow key={volume.id}>
                  <TableCell className="min-w-72">
                    <VolumeLocationLabel volume={volume} server={server} includeServer />
                  </TableCell>
                  <TableCell>{volume.created_by_name}</TableCell>
                  <TableCell><QuotaCell volume={volume} /></TableCell>
                  <TableCell className="text-muted-foreground">
                    {attached ? volume.attached_to.join(", ") : "Not attached"}
                  </TableCell>
                  <TableCell className="text-right">
                    <div className="flex justify-end gap-1.5">
                      <span className="inline-flex" title={cleanReason}>
                        <Button
                          variant="outline"
                          size="sm"
                          disabled={cleanReason !== undefined}
                          aria-describedby={cleanReason ? cleanDescriptionId : undefined}
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
                      </span>
                      <span className="inline-flex" title={deleteReason}>
                        <Button
                          variant="outline"
                          size="icon-sm"
                          aria-label={`Delete volume ${volume.name}`}
                          disabled={deleteReason !== undefined}
                          aria-describedby={deleteReason ? deleteDescriptionId : undefined}
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
                      </span>
                      {cleanReason ? (
                        <span id={cleanDescriptionId} className="sr-only">{cleanReason}</span>
                      ) : null}
                      {deleteReason ? (
                        <span id={deleteDescriptionId} className="sr-only">{deleteReason}</span>
                      ) : null}
                    </div>
                  </TableCell>
                </TableRow>
              )
            })}
          </TableBody>
        </Table>
      )}
    </div>
  )
}

function volumeActionDisabledReason(
  volume: ServerVolume,
  attached: boolean,
  pending: boolean,
  action: "clean" | "delete",
) {
  if (!volume.can_manage) {
    return `Only the volume creator or an administrator can ${action} this volume.`
  }
  if (attached) {
    return `This volume is attached to a deployment. Detach it before you can ${action} it.`
  }
  if (pending) return "Another volume operation is in progress."
  return undefined
}

function QuotaCell({ volume }: { volume: ServerVolume }) {
  const setQuota = useSetVolumeQuota()
  const serverValue = volume.quota_bytes == null ? "" : (volume.quota_bytes / 1024 ** 3).toFixed(1)
  const [value, setValue] = useState(serverValue)
  const [hasDraft, setHasDraft] = useState(false)
  const lastServerValue = useRef(serverValue)

  useEffect(() => {
    const serverValueChanged = lastServerValue.current !== serverValue
    lastServerValue.current = serverValue
    if (serverValueChanged && !hasDraft) setValue(serverValue)
  }, [hasDraft, serverValue])

  const submit = (quotaBytes: number | null) => {
    setQuota.mutate(
      { id: volume.id, quotaBytes },
      { onSuccess: () => setHasDraft(false) },
    )
  }

  return (
    <div className="min-w-48 space-y-1">
      <p className="text-xs text-muted-foreground">
        {volume.used_bytes == null ? "Measuring…" : formatFileSize(volume.used_bytes)} / {volume.quota_bytes == null ? "unlimited" : formatFileSize(volume.quota_bytes)}
      </p>
      {volume.can_manage ? (
        <div className="flex items-center gap-1">
          <Input
            className="h-7 w-20 text-xs"
            type="number"
            min="0.001"
            step="0.5"
            value={value}
            onChange={(event) => {
              setValue(event.target.value)
              setHasDraft(true)
            }}
            placeholder="GiB"
            aria-label={`Quota for ${volume.name} in GiB`}
          />
          <Button
            variant="outline"
            size="sm"
            className="h-7"
            disabled={setQuota.isPending || !value}
            onClick={() => submit(Math.round(Number(value) * 1024 ** 3))}
          >
            Set
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="h-7"
            disabled={setQuota.isPending || volume.quota_bytes == null}
            onClick={() => {
              setValue("")
              setHasDraft(true)
              submit(null)
            }}
          >
            ∞
          </Button>
        </div>
      ) : null}
    </div>
  )
}
