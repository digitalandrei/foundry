import { useEffect, useRef } from "react"
import { zodResolver } from "@hookform/resolvers/zod"
import { Loader2Icon } from "lucide-react"
import { useFieldArray, useForm, useFormState } from "react-hook-form"

import { DeployDialogFields } from "@/components/deploy-dialog-fields"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { useMe } from "@/hooks/use-auth"
import {
  useCreateDeployment,
  useDeploymentDetail,
  useImageMetadata,
  useReplaceDeployment,
  useServerVolumes,
} from "@/hooks/use-deployments"
import {
  MEM_UNLIMITED_GB,
  defaultPortKind,
  deploymentFormSchema,
  type DeploymentFormValues,
} from "@/lib/deployment-form"
import { formatSize } from "@/lib/format"
import type {
  CreateDeploymentRequest,
  DeploymentSummary,
  DragTagData,
  DropSlotData,
} from "@/lib/types"

/** A group deploy lands one container across N whole GPUs; the dialog needs
 * the name + member count + combined VRAM to summarise it. */
export interface DeployGroupTarget {
  id: string
  name: string
  serverId: string
  serverName: string
  memberCount: number
  vramMb: number
}

/** Where the deploy dialog is aimed — exactly one of a slot or a group. */
export interface DeployTarget {
  tag: DragTagData
  slot: DropSlotData | null
  group: DeployGroupTarget | null
  /** Present only when replacing an occupied single-use slot. */
  replaces: DeploymentSummary | null
}

/** Drag-drop deployment dialog (docs/UI-DESIGN.md; plans/phase-06.md). */
export function DeployDialog({
  target,
  onClose,
}: {
  target: DeployTarget | null
  onClose: () => void
}) {
  const create = useCreateDeployment()
  const replace = useReplaceDeployment()
  const serverId = target?.slot?.serverId ?? target?.group?.serverId ?? null
  const serverName = target?.slot?.serverName ?? target?.group?.serverName ?? ""
  const me = useMe()
  const appsDomain = me.data?.apps_domain ?? null
  const discovered = useImageMetadata(target?.tag.registryTagId ?? null)
  const replacingDetail = useDeploymentDetail(target?.replaces?.id ?? null)
  const volumes = useServerVolumes(serverId, discovered.data?.project_id ?? null, {
    slotId: target?.slot?.slotId,
    groupId: target?.group?.id,
  })

  const form = useForm<DeploymentFormValues>({
    resolver: zodResolver(deploymentFormSchema),
    defaultValues: {
      name: "",
      ports: [],
      env: [],
      volumes: [],
      mem_limit_gb: MEM_UNLIMITED_GB,
    },
  })
  const ports = useFieldArray({ control: form.control, name: "ports" })
  const envRows = useFieldArray({ control: form.control, name: "env" })
  const mounts = useFieldArray({ control: form.control, name: "volumes" })
  const { isDirty } = useFormState({ control: form.control })

  const targetKey = target?.slot
    ? `slot:${target.slot.slotId}`
    : target?.group
      ? `group:${target.group.id}`
      : null
  const prefillKey = target ? `${targetKey}:${target.tag.registryTagId}` : null
  const prefilled = useRef<string | null>(null)

  // Fresh target → reset and prefill once. A cancelled dialog never leaks
  // values (including secrets) into the next open.
  useEffect(() => {
    if (!prefillKey) {
      prefilled.current = null
      form.reset({
        name: "",
        ports: [],
        env: [],
        volumes: [],
        mem_limit_gb: MEM_UNLIMITED_GB,
      })
      return
    }
    if (prefilled.current === prefillKey) return
    if (isDirty) {
      prefilled.current = prefillKey
      return
    }
    if (discovered.isPending || (target?.replaces && replacingDetail.isPending)) return
    const discoveredVolumes = discovered.data?.volumes ?? []
    if (target?.replaces) {
      const retainedVolumes = (replacingDetail.data?.mounts ?? [])
        .filter((mount) => mount.volume_name !== null)
        .map((mount) => ({
          volume_id: mount.volume_id,
          volume_name: mount.volume_name!,
          container_path: mount.container_path,
          read_only: mount.read_only,
          visibility: mount.visibility ?? "PRIVATE" as const,
          placement: mount.placement ?? "SLOT" as const,
          purge_on_redeploy: mount.purge_on_redeploy,
        }))
      form.reset({
        name: target.replaces.name,
        ports: target.replaces.ports.map((port) => ({
          container_port: String(port.container_port),
          kind: port.kind,
          host_port: "",
        })),
        env: [],
        volumes: retainedVolumes.length > 0 ? retainedVolumes : discoveredVolumes,
        mem_limit_gb: MEM_UNLIMITED_GB,
      })
      prefilled.current = prefillKey
      return
    }
    const rows = (discovered.data?.ports ?? []).map((port) => ({
      container_port: String(port.container_port),
      kind: defaultPortKind(port, appsDomain !== null),
      host_port: "",
    }))
    form.reset({
      name: "",
      ports: rows,
      env: [],
      volumes: discoveredVolumes,
      mem_limit_gb: MEM_UNLIMITED_GB,
    })
    prefilled.current = prefillKey
  }, [
    prefillKey,
    target,
    discovered.isPending,
    discovered.data,
    replacingDetail.isPending,
    replacingDetail.data,
    appsDomain,
    isDirty,
    form,
  ])

  if (!target) return null
  const pending = create.isPending || replace.isPending
  const imageSize = target.tag.sizeBytes ?? discovered.data?.size_bytes
  const serverSlug =
    serverName
      .toLowerCase()
      .replace(/_/g, "-")
      .replace(/[^a-z0-9-]/g, "")
      .replace(/^-+|-+$/g, "") || "<server>"

  const onSubmit = form.handleSubmit((values) => {
    const request: CreateDeploymentRequest = {
      target: target.group
        ? { type: "group", gpu_group_id: target.group.id }
        : { type: "slot", slot_id: target.slot!.slotId },
      registry_tag_id: target.tag.registryTagId,
      name: values.name.trim() || undefined,
      ports: values.ports.map((port) => ({
        container_port: Number(port.container_port),
        kind: port.kind,
        host_port:
          port.kind === "HTTP" || port.kind === "HTTPS" || port.host_port === ""
            ? null
            : Number(port.host_port),
      })),
      env: values.env,
      volumes: values.volumes,
      mem_limit_mb:
        values.mem_limit_gb >= MEM_UNLIMITED_GB ? undefined : values.mem_limit_gb * 1024,
    }
    const done = {
      onSuccess: () => {
        form.reset()
        onClose()
      },
    }
    if (target.replaces) {
      replace.mutate({ oldId: target.replaces.id, req: request }, done)
    } else {
      create.mutate(request, done)
    }
  })

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-h-[85svh] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {target.replaces ? "Replace deployment" : "Deploy"} {target.tag.imageName}
            <Badge variant="secondary" className="ml-2 font-mono text-[11px]">
              {target.tag.tagName}
            </Badge>
          </DialogTitle>
          <DialogDescription>
            {target.group
              ? `→ ${target.group.serverName} · group ${target.group.name} · ${target.group.memberCount} GPUs · ${Math.round(target.group.vramMb / 1024)} GB combined`
              : `→ ${target.slot!.serverName} · slot ${target.slot!.slotName}`}
            {imageSize != null && imageSize > 0 ? ` · ${formatSize(imageSize)}` : ""}
          </DialogDescription>
        </DialogHeader>

        {target.replaces ? (
          <div className="rounded-md border border-slot-reserved/50 bg-slot-reserved/10 p-3 text-sm">
            This slot runs <span className="font-medium">{target.replaces.name}</span>{" "}
            (<span className="font-mono text-xs">{target.replaces.image_ref}</span>). Replacing
            stops and removes it first — its persistent volumes survive.
          </div>
        ) : null}

        {discovered.isPending ? (
          <div className="flex flex-col items-center gap-2 py-10 text-sm text-muted-foreground">
            <Loader2Icon className="size-5 animate-spin" aria-hidden />
            Inspecting image metadata…
          </div>
        ) : (
          <form onSubmit={onSubmit} noValidate className="flex flex-col gap-4">
            <DeployDialogFields
              form={form}
              ports={ports}
              envRows={envRows}
              mounts={mounts}
              imageName={target.tag.imageName}
              replacing={target.replaces !== null}
              appsDomain={appsDomain}
              serverSlug={serverSlug}
              discoveredPorts={discovered.data?.ports}
              discoveredVolumeCount={discovered.data?.volumes.length ?? 0}
              discoverySucceeded={discovered.isSuccess}
              availableVolumes={volumes.data ?? []}
            />
            <DialogFooter>
              <Button type="button" variant="outline" onClick={onClose} disabled={pending}>
                Cancel
              </Button>
              <Button
                type="submit"
                variant={target.replaces ? "destructive" : "default"}
                disabled={pending}
              >
                {pending ? "Submitting…" : target.replaces ? "Replace" : "Deploy"}
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  )
}
