import { normalizeContainerPath } from "@/lib/deployment-form"
import type { DeploymentFormValues } from "@/lib/deployment-form"
import type { ServerVolume, ServerVolumeAttachment } from "@/lib/types"
import { compareVolumeLocations, volumeLocation } from "@/lib/volume-locations"

export type VolumeSourceSuggestion = {
  volume: ServerVolume
  reasons: string[]
}

/** REMOVED/REPLACED rows are bounded history, not a current retained
 * reference. Every other deployment state still owns a durable binding that
 * can affect purge and cleanup policy. */
export function isRetainedVolumeAttachment(attachment: ServerVolumeAttachment) {
  return attachment.state !== "REMOVED" && attachment.state !== "REPLACED"
}

export function retainedVolumeAttachments(volume: ServerVolume) {
  return (volume.attachments ?? []).filter(isRetainedVolumeAttachment)
}

/** Compatibility fallback for a controller that predates rich attachment
 * records. New controllers use attachment IDs and states directly. */
export function volumeReferenceNames(volume: ServerVolume) {
  if (volume.attachments === undefined) return volume.attached_to
  return [...new Set(retainedVolumeAttachments(volume).map((attachment) => attachment.deployment_name))]
}

export function volumeReferenceSummary(volume: ServerVolume) {
  const retained = retainedVolumeAttachments(volume)
  const count = volume.attachments === undefined ? volume.attached_to.length : retained.length
  if (count > 0) {
    return `${count} active or retained binding${count === 1 ? "" : "s"}`
  }
  return volume.attachments && volume.attachments.length > 0
    ? "recent mapping history only"
    : "not currently referenced"
}

export function volumeAttachmentStateLabel(attachment: ServerVolumeAttachment) {
  return attachment.state.toLowerCase().replaceAll("_", " ")
}

/** Rank compatible roots without selecting one. A matching mount name is a
 * useful default signal; an earlier mapping to the same container destination
 * is stronger. The operator must still choose the source explicitly. */
export function suggestVolumeSources(
  volumes: ServerVolume[],
  mapping: Partial<DeploymentFormValues["volumes"][number]>,
  server: { name: string; hostname?: string | null },
): VolumeSourceSuggestion[] {
  const mountName = mapping.volume_name?.trim().toLocaleLowerCase()
  const destination = mapping.container_path ? normalizeContainerPath(mapping.container_path) : ""
  return volumes
    .map((volume) => {
      const reasons: string[] = []
      if (mountName && volume.name.toLocaleLowerCase() === mountName) {
        reasons.push("same mount name")
      }
      if (
        destination &&
        (volume.attachments ?? []).some(
          (attachment) => normalizeContainerPath(attachment.container_path) === destination,
        )
      ) {
        reasons.push("previously mapped to this container path")
      }
      return { volume, reasons }
    })
    .filter((suggestion) => suggestion.reasons.length > 0)
    .sort((left, right) => {
      const score = right.reasons.length - left.reasons.length
      if (score !== 0) return score
      return compareVolumeLocations(
        { volume: left.volume, location: volumeLocation(left.volume, server) },
        { volume: right.volume, location: volumeLocation(right.volume, server) },
      )
    })
}
