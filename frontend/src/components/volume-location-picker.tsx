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
import { formatSize } from "@/lib/format"
import { retainedVolumeAttachments, volumeReferenceSummary } from "@/lib/volume-mapping"

export function VolumeLocationPicker({
  value,
  volumes,
  server,
  onValueChange,
  ariaLabel,
  includeAutomatic = false,
  showDetails = false,
  unavailableValueLabel,
  suggestedVolumeIds = [],
  className,
}: {
  value: string
  volumes: ServerVolume[]
  server: { name: string; hostname?: string | null }
  onValueChange: (value: string) => void
  ariaLabel: string
  includeAutomatic?: boolean
  /** Deployment mappings need source safety context; file panes stay compact. */
  showDetails?: boolean
  /** Retains an already-selected legacy source in the trigger until an
   * operator explicitly chooses another source. */
  unavailableValueLabel?: string
  /** Non-destructive ranking supplied by a mapping row. Suggested roots are
   * grouped first but still require an explicit selection. */
  suggestedVolumeIds?: string[]
  className?: string
}) {
  const options = useMemo(() => {
    const locations = volumes
      .map((volume) => ({ volume, location: volumeLocation(volume, server) }))
      .sort(compareVolumeLocations)
    const suggested = new Set(suggestedVolumeIds)
    const roots: SearchablePickerOption[] = locations.map(({ volume, location }) => ({
      value: volume.id,
      label: location.breadcrumb,
      description: showDetails ? volumeOptionDescription(volume) : undefined,
      searchText: location.searchText,
      group: `${server.name} / ${location.placement}`,
      subgroup: location.project,
      content: <VolumeOption volume={volume} mount={location.mount} detailed={showDetails} />,
    }))
    const suggestedRoots = roots
      .filter((root) => suggested.has(root.value))
      .map((root) => ({ ...root, group: "Suggested for this mapping" }))
    const remainingRoots = roots.filter((root) => !suggested.has(root.value))
    const unavailable =
      unavailableValueLabel && value !== "automatic" && !volumes.some((volume) => volume.id === value)
        ? [{
            value,
            label: unavailableValueLabel,
            searchText: "current source unavailable legacy retained",
            group: "Current mapping",
            content: (
              <span>
                <span className="block text-xs font-medium">Current source unavailable / legacy</span>
                <span className="block text-[10px] text-muted-foreground">
                  Retained until you explicitly choose another source
                </span>
              </span>
            ),
          } satisfies SearchablePickerOption]
        : []
    const automatic = includeAutomatic
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
        ]
      : []
    return [...automatic, ...unavailable, ...suggestedRoots, ...remainingRoots]
  }, [
    includeAutomatic,
    server,
    showDetails,
    suggestedVolumeIds,
    unavailableValueLabel,
    value,
    volumes,
  ])

  return (
    <SearchablePicker
      value={value}
      options={options}
      onValueChange={onValueChange}
      ariaLabel={ariaLabel}
      placeholder="Choose a volume"
      searchPlaceholder="Search server, project, mount, path, owner, or use…"
      emptyMessage="No volume locations match this search."
      className={className}
    />
  )
}

function VolumeOption({
  volume,
  mount,
  detailed,
}: {
  volume: ServerVolume
  mount: string
  detailed: boolean
}) {
  const usage = volume.used_bytes === null ? "usage pending" : formatSize(volume.used_bytes)
  const quota = volume.quota_bytes === null ? null : ` / ${formatSize(volume.quota_bytes)}`
  const reference = volumeReferenceSummary(volume)
  const hasRetainedReference = volume.attachments === undefined
    ? volume.attached_to.length > 0
    : retainedVolumeAttachments(volume).length > 0
  return (
    <span className="flex min-w-0 items-center justify-between gap-3">
      <span className="min-w-0">
        <span className="block truncate font-mono text-xs">{mount}</span>
        {detailed ? (
          <span className="block truncate text-[10px] text-muted-foreground">
            {usage}{quota} · {reference} · owner {volume.created_by_name}
          </span>
        ) : null}
      </span>
      {hasRetainedReference ? (
        <span className="shrink-0 text-[10px] text-slot-reserved">referenced</span>
      ) : null}
    </span>
  )
}

function volumeOptionDescription(volume: ServerVolume) {
  const usage = volume.used_bytes === null ? "Usage pending" : `Usage ${formatSize(volume.used_bytes)}`
  const quota = volume.quota_bytes === null ? "no quota" : `quota ${formatSize(volume.quota_bytes)}`
  return `${usage}; ${quota}; ${volumeReferenceSummary(volume)}; owner ${volume.created_by_name}`
}
