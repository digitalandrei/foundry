import type { ServerVolume } from "@/lib/types"

export type VolumeLocation = {
  placement: string
  project: string
  mount: string
  breadcrumb: string
  searchText: string
  sortKey: string
}

function fallbackId(id: string | null) {
  return id ? id.slice(0, 8) : "unknown"
}

export function volumePlacementLabel(volume: ServerVolume) {
  if (volume.placement === "SERVER") return "Shared"
  if (volume.gpu_group_id) {
    return `Group ${volume.group_name ?? fallbackId(volume.gpu_group_id)}`
  }
  return `Slot ${volume.slot_name ?? fallbackId(volume.slot_id)}`
}

/** Canonical operator-facing hierarchy. `project_name` is the deployment
 * name, never a GitLab project; the wording stays explicit in every picker. */
export function volumeLocation(
  volume: ServerVolume,
  server: { name: string; hostname?: string | null },
): VolumeLocation {
  const placement = volumePlacementLabel(volume)
  const project = `Project ${volume.project_name}`
  const mount = `Mount ${volume.name}`
  const breadcrumb = `${server.name} / ${placement} / ${project} / ${mount}`
  const aliases = volume.placement === "SERVER" ? "server shared global" : "slot local"
  const attachmentTerms = (volume.attachments ?? []).flatMap((attachment) => [
    attachment.deployment_name,
    attachment.container_path,
    attachment.state,
    attachment.read_only ? "read only ro" : "read write rw",
    attachment.purge_on_redeploy ? "purge purges redeploy" : "",
  ])
  return {
    placement,
    project,
    mount,
    breadcrumb,
    searchText: [
      server.name,
      server.hostname,
      placement,
      aliases,
      "project",
      volume.project_name,
      "mount volume",
      volume.name,
      "owner",
      volume.created_by_name,
      "usage quota",
      ...volume.attached_to,
      ...attachmentTerms,
    ]
      .filter(Boolean)
      .join(" "),
    sortKey: [
      volume.placement === "SERVER" ? "0" : volume.gpu_group_id ? "2" : "1",
      placement,
      volume.project_name,
      volume.name,
    ]
      .join("\0")
      .toLocaleLowerCase(),
  }
}

export function compareVolumeLocations(
  left: { volume: ServerVolume; location: VolumeLocation },
  right: { volume: ServerVolume; location: VolumeLocation },
) {
  return left.location.sortKey.localeCompare(right.location.sortKey) ||
    left.volume.id.localeCompare(right.volume.id)
}
