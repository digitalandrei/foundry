import { useEffect, useMemo } from "react"
import { ArrowRightIcon, DatabaseIcon, LinkIcon, TriangleAlertIcon } from "lucide-react"
import type {
  UseFieldArrayReturn,
  UseFormRegisterReturn,
  UseFormReturn,
} from "react-hook-form"

import { Badge } from "@/components/ui/badge"
import { Checkbox } from "@/components/ui/checkbox"
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { VolumeLocationPicker } from "@/components/volume-location-picker"
import type { DeploymentFormValues } from "@/lib/deployment-form"
import { formatSize } from "@/lib/format"
import { volumeLocation } from "@/lib/volume-locations"
import {
  isRetainedVolumeAttachment,
  retainedVolumeAttachments,
  suggestVolumeSources,
  volumeAttachmentStateLabel,
  volumeReferenceNames,
  volumeReferenceSummary,
} from "@/lib/volume-mapping"
import type { ServerVolume, VolumePlacement } from "@/lib/types"

type MountValues = Partial<DeploymentFormValues["volumes"][number]>

type PersistentMountRowProps = {
  index: number
  fieldId: string
  form: UseFormReturn<DeploymentFormValues>
  mounts: UseFieldArrayReturn<DeploymentFormValues, "volumes">
  mount: MountValues
  selectedId: string | null
  selectedVolume: ServerVolume | null
  availableVolumes: ServerVolume[]
  storageServer: { name: string; hostname: string | null }
  projectName: string
  loading: boolean
  error: string | null
  replacementDeploymentId: string | null
}

/** One deliberate Docker bind mapping. The backing root is immutable when an
 * existing volume is selected; only the container-facing mapping is edited. */
export function PersistentMountRow({
  index,
  fieldId,
  form,
  mounts,
  mount,
  selectedId,
  selectedVolume,
  availableVolumes,
  storageServer,
  projectName,
  loading,
  error,
  replacementDeploymentId,
}: PersistentMountRowProps) {
  const existingSource = selectedId !== null
  const selectedLocation = selectedVolume
    ? volumeLocation(selectedVolume, storageServer)
    : null
  const suggestions = useMemo(
    () => (existingSource ? [] : suggestVolumeSources(availableVolumes, mount, storageServer)),
    [availableVolumes, existingSource, mount, storageServer],
  )
  const nameRegistration = form.register(`volumes.${index}.volume_name`)
  const retainedAttachments = selectedVolume ? retainedVolumeAttachments(selectedVolume) : []
  const referenceNames = selectedVolume ? volumeReferenceNames(selectedVolume) : []
  const existingPurgeAttachment = retainedAttachments.find(
    (attachment) => attachment.purge_on_redeploy,
  )
  const unrelatedAttachments = retainedAttachments.length > 0
    ? retainedAttachments.filter(
        (attachment) => attachment.deployment_id !== replacementDeploymentId,
      )
    : (selectedVolume?.attached_to ?? []).filter(
        (deploymentName) =>
          replacementDeploymentId === null || deploymentName !== projectName,
      )
  const purgeBlocked = existingSource && unrelatedAttachments.length > 0

  // Keep the form aligned with the controller's destructive-sharing guard,
  // including when volume details arrive after replacement defaults.
  useEffect(() => {
    if (purgeBlocked && mount.purge_on_redeploy) {
      form.setValue(`volumes.${index}.purge_on_redeploy`, false, {
        shouldDirty: true,
        shouldValidate: true,
      })
    }
  }, [form, index, mount.purge_on_redeploy, purgeBlocked])

  const chooseSource = (value: string) => {
    if (value === "automatic") {
      // Keep the previous logical name/placement as a useful starting point.
      // Removing only the ID deliberately switches back to normal canonical
      // create/reuse semantics under this deployment's project namespace.
      form.setValue(`volumes.${index}.volume_id`, null, { shouldDirty: true })
      return
    }
    const volume = availableVolumes.find((candidate) => candidate.id === value)
    if (!volume) return
    // The source's name and placement describe its immutable identity. The
    // destination below remains independent, as Docker bind mounts allow.
    form.setValue(`volumes.${index}.volume_id`, volume.id, { shouldDirty: true })
    form.setValue(`volumes.${index}.volume_name`, volume.name, { shouldDirty: true })
    form.setValue(`volumes.${index}.placement`, volume.placement, { shouldDirty: true })
    if (unrelatedRetainedBindings(volume, replacementDeploymentId, projectName)) {
      form.setValue(`volumes.${index}.purge_on_redeploy`, false, { shouldDirty: true })
    }
  }

  return (
    <section
      id={`persistent-mount-${fieldId}`}
      className="space-y-3 rounded-lg border bg-card p-3"
      aria-label={`Persistent mount ${index + 1}`}
    >
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div>
          <p className="text-sm font-medium">Mount {index + 1}</p>
          <p className="text-xs text-muted-foreground">Storage source → container destination</p>
        </div>
        <Badge variant={existingSource ? "secondary" : "outline"}>
          {existingSource ? "Existing root" : "Automatic root"}
        </Badge>
      </div>

      <Field>
        <FieldLabel>Storage source</FieldLabel>
        <VolumeLocationPicker
          value={selectedId ?? "automatic"}
          volumes={availableVolumes}
          server={storageServer}
          onValueChange={chooseSource}
          ariaLabel={`Storage source for mount ${index + 1}`}
          includeAutomatic
          showDetails
          suggestedVolumeIds={suggestions.map((suggestion) => suggestion.volume.id)}
          unavailableValueLabel={
            existingSource && !selectedVolume
              ? unavailableSourceLabel(mount.placement ?? "SLOT", mount.volume_name ?? "")
              : undefined
          }
        />
        <FieldDescription>
          Search roots previously used in this target slot/group or shared on this server. A
          selected root is bind-mounted as-is; it is never copied or renamed.
        </FieldDescription>
        {suggestions.length > 0 ? (
          <p className="text-xs text-muted-foreground">
            Suggested: {suggestions.slice(0, 3).map((suggestion) => (
              <span key={suggestion.volume.id} className="mr-1">
                <span className="font-medium text-foreground">{suggestion.volume.project_name}/{suggestion.volume.name}</span>
                {" "}({suggestion.reasons.join(", ")})
              </span>
            ))}
            — selecting a root is always explicit.
          </p>
        ) : null}
        {loading ? (
          <p className="text-xs text-muted-foreground" role="status">
            Loading reusable roots…
          </p>
        ) : error ? (
          <p className="text-xs text-destructive" role="alert">
            Reusable roots could not be loaded: {error}. Automatic storage is still available.
          </p>
        ) : availableVolumes.length === 0 ? (
          <p className="text-xs text-muted-foreground">
            No compatible existing root is available here. Automatic storage will create or reuse
            this deployment’s own root.
          </p>
        ) : null}
      </Field>

      {existingSource ? (
        <ExistingSourceSummary
          volume={selectedVolume}
          location={selectedLocation?.breadcrumb ?? null}
          loading={loading}
          mount={mount}
        />
      ) : (
        <AutomaticSourceFields
          index={index}
          form={form}
          nameRegistration={nameRegistration}
          projectName={projectName}
          mount={mount}
        />
      )}

      <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] sm:items-end">
        <div className="rounded-md border border-dashed bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
          <span className="flex items-center gap-1.5 font-medium text-foreground">
            <DatabaseIcon className="size-3.5" aria-hidden />
            {existingSource ? "Selected storage root" : "Automatic storage root"}
          </span>
          <span className="mt-0.5 block truncate font-mono" title={selectedLocation?.breadcrumb}>
            {selectedLocation?.breadcrumb ?? automaticRootPreview(projectName, mount)}
          </span>
        </div>
        <ArrowRightIcon className="mx-auto size-4 rotate-90 text-muted-foreground sm:mb-2 sm:rotate-0" aria-hidden />
        <Field>
          <FieldLabel htmlFor={`volume-path-${index}`}>Container destination</FieldLabel>
          <Input
            id={`volume-path-${index}`}
            placeholder="/data"
            autoComplete="off"
            {...form.register(`volumes.${index}.container_path`)}
          />
          <FieldDescription>Editable Docker bind-mount destination.</FieldDescription>
          {form.formState.errors.volumes?.[index]?.container_path ? (
            <FieldError>{form.formState.errors.volumes[index]?.container_path?.message}</FieldError>
          ) : null}
        </Field>
      </div>

      <div className="flex flex-wrap gap-x-4 gap-y-2 text-xs text-muted-foreground">
        <label className="flex items-center gap-1.5">
          <Checkbox
            checked={mount.read_only ?? false}
            onCheckedChange={(value) =>
              form.setValue(`volumes.${index}.read_only`, value === true, { shouldDirty: true })
            }
          />
          Read-only mount
        </label>
        <label className="flex items-center gap-1.5">
          <Checkbox
            checked={mount.purge_on_redeploy ?? false}
            disabled={purgeBlocked}
            onCheckedChange={(value) =>
              form.setValue(`volumes.${index}.purge_on_redeploy`, value === true, {
                shouldDirty: true,
              })
            }
          />
          {purgeBlocked ? "Purge unavailable while this root is referenced" : "Purge before future redeploys"}
        </label>
        <button
          type="button"
          className="ml-auto inline-flex items-center gap-1 text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
          onClick={() => mounts.remove(index)}
          aria-label={`Remove persistent mount ${index + 1}`}
        >
          Remove
        </button>
      </div>
      {referenceNames.length > 0 ? (
        <div
          className="flex items-start gap-2 rounded-md border border-slot-reserved/50 bg-slot-reserved/10 p-2 text-xs"
          role="note"
        >
          <TriangleAlertIcon className="size-4 shrink-0 text-slot-reserved" aria-hidden />
          <span>
            Referenced by <span className="font-medium">{referenceNames.join(", ")}</span>.
            {purgeBlocked ? (
              <> Purge is disabled while another active or retained deployment references this root.</>
            ) : mount.purge_on_redeploy ? (
              <> Purge is enabled and will erase this root before a future redeploy.</>
            ) : (
              <> This mapping shares the same files whenever those deployments run; purge remains off by default.</>
            )}
            {existingPurgeAttachment ? (
              <>
                {" "}The {volumeAttachmentStateLabel(existingPurgeAttachment)} {existingPurgeAttachment.deployment_name} mapping
                is already configured to purge on redeploy; review that policy before sharing or changing this root.
              </>
            ) : null}
          </span>
        </div>
      ) : null}
    </section>
  )
}

function AutomaticSourceFields({
  index,
  form,
  nameRegistration,
  projectName,
  mount,
}: {
  index: number
  form: UseFormReturn<DeploymentFormValues>
  nameRegistration: UseFormRegisterReturn
  projectName: string
  mount: MountValues
}) {
  return (
    <div className="grid gap-2 sm:grid-cols-2">
      <Field>
        <FieldLabel htmlFor={`volume-name-${index}`}>New mount name</FieldLabel>
        <Input
          id={`volume-name-${index}`}
          placeholder="models"
          autoComplete="off"
          {...nameRegistration}
        />
        <FieldDescription>
          Project {projectName || "<deployment name>"} / Mount {mount.volume_name || "<name>"}
        </FieldDescription>
        {form.formState.errors.volumes?.[index]?.volume_name ? (
          <FieldError>use letters, digits, dash, or underscore</FieldError>
        ) : null}
      </Field>
      <Field>
        <FieldLabel>Automatic placement</FieldLabel>
        <Select
          value={mount.placement ?? "SLOT"}
          onValueChange={(value) =>
            form.setValue(`volumes.${index}.placement`, value as VolumePlacement, {
              shouldDirty: true,
            })
          }
        >
          <SelectTrigger><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem value="SLOT">This slot/group</SelectItem>
            <SelectItem value="SERVER">Shared on this server</SelectItem>
          </SelectContent>
        </Select>
        <FieldDescription>Automatic roots are scoped by this placement.</FieldDescription>
      </Field>
    </div>
  )
}

function ExistingSourceSummary({
  volume,
  location,
  loading,
  mount,
}: {
  volume: ServerVolume | null
  location: string | null
  loading: boolean
  mount: MountValues
}) {
  if (!volume) {
    return (
      <div className="rounded-md border border-slot-reserved/50 bg-slot-reserved/10 p-2 text-xs" role="status">
        {loading ? (
          "Loading the selected root’s details…"
        ) : (
          <>
            <span className="font-medium">Current source unavailable / legacy.</span>{" "}
            {unavailableSourceLabel(mount.placement ?? "SLOT", mount.volume_name ?? "")} remains selected and will
            not be recreated. Choose Automatic new/reuse only to intentionally remap it.
          </>
        )}
      </div>
    )
  }
  const usage = volume.used_bytes === null ? "Usage pending" : formatSize(volume.used_bytes)
  const quota = volume.quota_bytes === null ? "No quota" : `of ${formatSize(volume.quota_bytes)}`
  const mappings = volume.attachments ?? []
  return (
    <div className="rounded-md border bg-muted/30 p-2.5 text-xs">
      <div className="flex flex-wrap items-center gap-2">
        <LinkIcon className="size-3.5 text-muted-foreground" aria-hidden />
        <span className="font-medium">Existing source stays authoritative</span>
        <span className="font-mono text-muted-foreground" title={location ?? undefined}>
          {location}
        </span>
      </div>
      <dl className="mt-2 grid gap-x-4 gap-y-1 text-muted-foreground sm:grid-cols-3">
        <div>
          <dt className="sr-only">Usage</dt>
          <dd>{usage} {quota}</dd>
        </div>
        <div>
          <dt className="sr-only">Owner</dt>
          <dd>Owner: {volume.created_by_name}</dd>
        </div>
        <div>
          <dt className="sr-only">Attachment status</dt>
          <dd>{volumeReferenceSummary(volume)}</dd>
        </div>
      </dl>
      {mappings.length > 0 ? (
        <div className="mt-2 border-t pt-2">
          <p className="font-medium text-foreground">Known mappings</p>
          <ul className="mt-1 space-y-1 text-muted-foreground" aria-label="Known volume mappings">
            {mappings.map((attachment) => {
              const retained = isRetainedVolumeAttachment(attachment)
              return (
                <li key={`${attachment.deployment_id}:${attachment.container_path}`}>
                  <span className="font-medium text-foreground">{attachment.deployment_name}</span>
                  {" · "}{attachment.container_path}{" · "}
                  {attachment.read_only ? "read-only" : "read-write"}{" · "}
                  <span className={retained ? "text-slot-reserved" : undefined}>
                    {volumeAttachmentStateLabel(attachment)}
                  </span>{attachment.purge_on_redeploy ? " · purges on redeploy" : ""}
                </li>
              )
            })}
          </ul>
        </div>
      ) : null}
    </div>
  )
}

function automaticRootPreview(
  projectName: string,
  volume: MountValues,
) {
  const placement = volume.placement === "SERVER" ? "Shared" : "This slot/group"
  return `${placement} / Project ${projectName || "<deployment name>"} / Mount ${volume.volume_name || "<name>"}`
}

function unavailableSourceLabel(placement: VolumePlacement, name: string) {
  return `${placement === "SERVER" ? "Shared" : "This slot/group"} / Mount ${name || "<name>"}`
}

function unrelatedRetainedBindings(
  volume: ServerVolume,
  replacementDeploymentId: string | null,
  projectName: string,
) {
  const retained = retainedVolumeAttachments(volume)
  if (retained.length > 0) {
    return retained.some(
      (attachment) => attachment.deployment_id !== replacementDeploymentId,
    )
  }
  return volume.attached_to.some(
    (deploymentName) =>
      replacementDeploymentId === null || deploymentName !== projectName,
  )
}
