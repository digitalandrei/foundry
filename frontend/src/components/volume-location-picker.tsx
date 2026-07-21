import { useMemo } from "react"

import {
  SearchablePicker,
  type SearchablePickerOption,
} from "@/components/searchable-picker"
import type { ServerVolume } from "@/lib/types"
import {
  compareVolumeLocations,
  volumeLocation,
} from "@/lib/volume-locations"

export function VolumeLocationPicker({
  value,
  volumes,
  server,
  onValueChange,
  ariaLabel,
  includeAutomatic = false,
  className,
}: {
  value: string
  volumes: ServerVolume[]
  server: { name: string; hostname?: string | null }
  onValueChange: (value: string) => void
  ariaLabel: string
  includeAutomatic?: boolean
  className?: string
}) {
  const options = useMemo(() => {
    const locations = volumes
      .map((volume) => ({ volume, location: volumeLocation(volume, server) }))
      .sort(compareVolumeLocations)
    const roots: SearchablePickerOption[] = locations.map(({ volume, location }) => ({
      value: volume.id,
      label: location.breadcrumb,
      searchText: location.searchText,
      group: `${server.name} / ${location.placement}`,
      subgroup: location.project,
      content: (
        <span className="flex min-w-0 items-center justify-between gap-3">
          <span className="truncate font-mono text-xs">{location.mount}</span>
          {volume.attached_to.length > 0 ? (
            <span className="shrink-0 text-[10px] text-muted-foreground">
              attached · {volume.attached_to.length}
            </span>
          ) : null}
        </span>
      ),
    }))
    return includeAutomatic
      ? [
          {
            value: "automatic",
            label: "Create or reuse automatically",
            searchText: "automatic create reuse policy",
            content: (
              <span>
                <span className="block text-xs font-medium">Create/reuse automatically</span>
                <span className="block text-[10px] text-muted-foreground">
                  Uses this deploy name, placement, and mount name
                </span>
              </span>
            ),
          },
          ...roots,
        ]
      : roots
  }, [includeAutomatic, server, volumes])

  return (
    <SearchablePicker
      value={value}
      options={options}
      onValueChange={onValueChange}
      ariaLabel={ariaLabel}
      placeholder="Choose a volume"
      searchPlaceholder="Search server, placement, project, or volume…"
      emptyMessage="No volume locations match this search."
      className={className}
    />
  )
}
