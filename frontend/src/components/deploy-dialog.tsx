import { useFieldArray, useForm } from "react-hook-form"
import { zodResolver } from "@hookform/resolvers/zod"
import { PlusIcon, Trash2Icon } from "lucide-react"
import { z } from "zod"

import { useCreateDeployment, useReplaceDeployment, useServerVolumes } from "@/hooks/use-deployments"
import { formatSize } from "@/lib/format"
import type { DeploymentSummary, DragTagData, DropSlotData } from "@/lib/types"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Separator } from "@/components/ui/separator"

const portRow = z.object({
  container_port: z
    .string()
    .regex(/^\d+$/, "number required")
    .refine((v) => +v >= 1 && +v <= 65535, "1–65535"),
  kind: z.enum(["TCP", "UDP"]),
  host_port: z
    .string()
    .regex(/^\d*$/, "number")
    .refine((v) => v === "" || (+v >= 20000 && +v <= 29999), "20000–29999"),
})
const envRow = z.object({
  key: z
    .string()
    .regex(/^[A-Za-z_][A-Za-z0-9_]*$/, "letters/digits/underscore")
    .max(128),
  value: z.string().max(4096),
  is_secret: z.boolean(),
})
const volumeRow = z.object({
  volume_name: z
    .string()
    .regex(/^[A-Za-z0-9][A-Za-z0-9_-]*$/, "alphanumeric/dash/underscore")
    .max(63),
  container_path: z.string().startsWith("/", "must be absolute").max(255),
  read_only: z.boolean(),
})
const schema = z.object({
  name: z
    .string()
    .regex(/^$|^[A-Za-z0-9][A-Za-z0-9_-]*$/, "alphanumeric/dash/underscore")
    .max(63),
  ports: z.array(portRow).max(32),
  env: z.array(envRow).max(64),
  volumes: z.array(volumeRow).max(16),
})
type FormValues = z.infer<typeof schema>

export interface DeployTarget {
  tag: DragTagData
  slot: DropSlotData
  /** Present when dropping on an occupied slot → replacement flow. */
  replaces: DeploymentSummary | null
}

/** Drag-drop deployment dialog (docs/UI-DESIGN.md; plans/phase-06.md):
 * per-port kind (HTTP/S arrives with the proxy build), env with
 * secrets, persistent volumes (per-user, reusable). */
export function DeployDialog({
  target,
  onClose,
}: {
  target: DeployTarget | null
  onClose: () => void
}) {
  const create = useCreateDeployment()
  const replace = useReplaceDeployment()
  const volumes = useServerVolumes(target?.slot.serverId ?? null)

  const form = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: { name: "", ports: [], env: [], volumes: [] },
  })
  const ports = useFieldArray({ control: form.control, name: "ports" })
  const env = useFieldArray({ control: form.control, name: "env" })
  const mounts = useFieldArray({ control: form.control, name: "volumes" })

  if (!target) return null
  const pending = create.isPending || replace.isPending

  const onSubmit = form.handleSubmit((values) => {
    const req = {
      slot_id: target.slot.slotId,
      registry_tag_id: target.tag.registryTagId,
      name: values.name.trim() || undefined,
      ports: values.ports.map((p) => ({
        container_port: Number(p.container_port),
        kind: p.kind,
        host_port: p.host_port === "" ? null : Number(p.host_port),
      })),
      env: values.env,
      volumes: values.volumes,
    }
    const done = {
      onSuccess: () => {
        form.reset()
        onClose()
      },
    }
    if (target.replaces) {
      replace.mutate({ oldId: target.replaces.id, req }, done)
    } else {
      create.mutate(req, done)
    }
  })

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-h-[85svh] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {target.replaces ? "Replace deployment" : "Deploy"} {target.tag.imageName}
            <Badge variant="secondary" className="ml-2 font-mono text-[11px]">
              {target.tag.tagName}
            </Badge>
          </DialogTitle>
          <DialogDescription>
            → {target.slot.serverName} · slot {target.slot.slotName}
            {target.tag.sizeBytes != null ? ` · ${formatSize(target.tag.sizeBytes)}` : ""}
          </DialogDescription>
        </DialogHeader>

        {target.replaces ? (
          <div className="rounded-md border border-slot-reserved/50 bg-slot-reserved/10 p-3 text-sm">
            This slot runs <span className="font-medium">{target.replaces.name}</span>{" "}
            (<span className="font-mono text-xs">{target.replaces.image_ref}</span>).
            Replacing stops and removes it first — its persistent volumes survive.
          </div>
        ) : null}

        <form onSubmit={onSubmit} noValidate className="flex flex-col gap-4">
          <Field data-invalid={!!form.formState.errors.name}>
            <FieldLabel htmlFor="dep-name">Name</FieldLabel>
            <Input
              id="dep-name"
              placeholder={`${target.tag.imageName}-auto`}
              autoComplete="off"
              {...form.register("name")}
            />
            <FieldDescription>Optional — generated when empty.</FieldDescription>
            {form.formState.errors.name ? (
              <FieldError>{form.formState.errors.name.message}</FieldError>
            ) : null}
          </Field>

          <Separator />
          <SectionHeader
            title="Ports"
            hint="TCP/UDP map directly onto the server IP; HTTP/S proxying arrives with the apps domain."
            onAdd={() => ports.append({ container_port: "8080", kind: "TCP", host_port: "" })}
          />
          {ports.fields.map((field, i) => (
            <div key={field.id} className="flex items-start gap-2">
              <Field className="flex-1">
                <FieldLabel htmlFor={`port-c-${i}`}>Container</FieldLabel>
                <Input
                  id={`port-c-${i}`}
                  inputMode="numeric"
                  {...form.register(`ports.${i}.container_port`)}
                />
                {form.formState.errors.ports?.[i]?.container_port ? (
                  <FieldError>invalid</FieldError>
                ) : null}
              </Field>
              <Field className="w-24">
                <FieldLabel>Kind</FieldLabel>
                <Select
                  value={form.watch(`ports.${i}.kind`)}
                  onValueChange={(v) => form.setValue(`ports.${i}.kind`, v as "TCP" | "UDP")}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="TCP">TCP</SelectItem>
                    <SelectItem value="UDP">UDP</SelectItem>
                  </SelectContent>
                </Select>
              </Field>
              <Field className="flex-1">
                <FieldLabel htmlFor={`port-h-${i}`}>Host (optional)</FieldLabel>
                <Input
                  id={`port-h-${i}`}
                  inputMode="numeric"
                  placeholder="auto"
                  {...form.register(`ports.${i}.host_port`)}
                />
                {form.formState.errors.ports?.[i]?.host_port ? (
                  <FieldError>20000–29999</FieldError>
                ) : null}
              </Field>
              <RemoveRowButton onClick={() => ports.remove(i)} />
            </div>
          ))}

          <Separator />
          <SectionHeader
            title="Environment"
            onAdd={() => env.append({ key: "", value: "", is_secret: false })}
          />
          {env.fields.map((field, i) => (
            <div key={field.id} className="flex items-start gap-2">
              <Field className="flex-1">
                <Input placeholder="KEY" autoComplete="off" {...form.register(`env.${i}.key`)} />
                {form.formState.errors.env?.[i]?.key ? <FieldError>invalid key</FieldError> : null}
              </Field>
              <Field className="flex-1">
                <Input
                  placeholder="value"
                  autoComplete="off"
                  type={form.watch(`env.${i}.is_secret`) ? "password" : "text"}
                  {...form.register(`env.${i}.value`)}
                />
              </Field>
              <label className="flex h-9 items-center gap-1.5 text-xs text-muted-foreground">
                <Checkbox
                  checked={form.watch(`env.${i}.is_secret`)}
                  onCheckedChange={(v) => form.setValue(`env.${i}.is_secret`, v === true)}
                />
                secret
              </label>
              <RemoveRowButton onClick={() => env.remove(i)} />
            </div>
          ))}

          <Separator />
          <SectionHeader
            title="Persistent storage"
            hint={`Volumes live at /storage/containers/<you>/<name>, survive container removal, and can be remounted later.${
              volumes.data?.length
                ? ` Yours on this server: ${volumes.data.map((v) => v.name).join(", ")}.`
                : ""
            }`}
            onAdd={() => mounts.append({ volume_name: "", container_path: "/data", read_only: false })}
          />
          {mounts.fields.map((field, i) => (
            <div key={field.id} className="flex items-start gap-2">
              <Field className="flex-1">
                <Input
                  placeholder="volume name"
                  autoComplete="off"
                  list="existing-volumes"
                  {...form.register(`volumes.${i}.volume_name`)}
                />
                {form.formState.errors.volumes?.[i]?.volume_name ? (
                  <FieldError>invalid name</FieldError>
                ) : null}
              </Field>
              <Field className="flex-1">
                <Input
                  placeholder="/data"
                  autoComplete="off"
                  {...form.register(`volumes.${i}.container_path`)}
                />
                {form.formState.errors.volumes?.[i]?.container_path ? (
                  <FieldError>absolute path</FieldError>
                ) : null}
              </Field>
              <label className="flex h-9 items-center gap-1.5 text-xs text-muted-foreground">
                <Checkbox
                  checked={form.watch(`volumes.${i}.read_only`)}
                  onCheckedChange={(v) => form.setValue(`volumes.${i}.read_only`, v === true)}
                />
                ro
              </label>
              <RemoveRowButton onClick={() => mounts.remove(i)} />
            </div>
          ))}
          <datalist id="existing-volumes">
            {(volumes.data ?? []).map((v) => (
              <option key={v.id} value={v.name} />
            ))}
          </datalist>

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
      </DialogContent>
    </Dialog>
  )
}

function SectionHeader({
  title,
  hint,
  onAdd,
}: {
  title: string
  hint?: string
  onAdd: () => void
}) {
  return (
    <div>
      <div className="flex items-center justify-between">
        <p className="text-sm font-medium">{title}</p>
        <Button type="button" variant="ghost" size="sm" onClick={onAdd}>
          <PlusIcon className="size-3.5" aria-hidden /> Add
        </Button>
      </div>
      {hint ? <p className="text-xs text-muted-foreground">{hint}</p> : null}
    </div>
  )
}

function RemoveRowButton({ onClick }: { onClick: () => void }) {
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon"
      className="mt-0 size-9 shrink-0"
      onClick={onClick}
      aria-label="Remove row"
    >
      <Trash2Icon className="size-3.5" aria-hidden />
    </Button>
  )
}
