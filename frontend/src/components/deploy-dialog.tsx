import { useEffect, useRef } from "react"
import { useFieldArray, useForm, useFormState } from "react-hook-form"
import { zodResolver } from "@hookform/resolvers/zod"
import { PlusIcon, Trash2Icon } from "lucide-react"
import { z } from "zod"

import { useMe } from "@/hooks/use-auth"
import {
  useCreateDeployment,
  useExposedPorts,
  useReplaceDeployment,
  useServerVolumes,
} from "@/hooks/use-deployments"
import { formatSize } from "@/lib/format"
import type {
  DeploymentSummary,
  DragTagData,
  DropSlotData,
  ExposedPort,
  PortKind,
} from "@/lib/types"
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
  kind: z.enum(["HTTP", "HTTPS", "TCP", "UDP"]),
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

/** Default port kind for discovered EXPOSE entries: web-looking TCP
 * ports become HTTP/S vhosts when the apps domain is configured. */
const WEB_PORTS = new Set([80, 3000, 5000, 7860, 8000, 8080, 8081, 8501, 8888])
function defaultKind(p: ExposedPort, appsEnabled: boolean): PortKind {
  if (p.protocol === "udp") return "UDP"
  if (!appsEnabled) return "TCP"
  if (p.container_port === 443 || p.container_port === 8443) return "HTTPS"
  return WEB_PORTS.has(p.container_port) ? "HTTP" : "TCP"
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
  const me = useMe()
  const appsDomain = me.data?.apps_domain ?? null
  const discovered = useExposedPorts(target?.tag.registryTagId ?? null)

  const form = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: { name: "", ports: [], env: [], volumes: [] },
  })
  const ports = useFieldArray({ control: form.control, name: "ports" })
  const env = useFieldArray({ control: form.control, name: "env" })
  const mounts = useFieldArray({ control: form.control, name: "volumes" })
  // Render-level subscription — reading form.formState inside an effect
  // is unsubscribed and goes stale (review finding).
  const { isDirty } = useFormState({ control: form.control })

  // Fresh target → reset and prefill (once per open; never over the
  // user's own edits). Replacements inherit the outgoing deployment's
  // name + port layout so the app URL survives the swap; fresh deploys
  // prefill from the image's EXPOSE list.
  const prefillKey = target ? `${target.slot.slotId}:${target.tag.registryTagId}` : null
  const prefilled = useRef<string | null>(null)
  useEffect(() => {
    if (!prefillKey) {
      // Dialog closed — drop leftovers so a cancelled open can't leak
      // values (incl. secrets) into the next one (review finding).
      prefilled.current = null
      form.reset({ name: "", ports: [], env: [], volumes: [] })
      return
    }
    if (prefilled.current === prefillKey) return
    if (isDirty) {
      prefilled.current = prefillKey
      return
    }
    if (target?.replaces) {
      form.reset({
        name: target.replaces.name,
        ports: target.replaces.ports.map((p) => ({
          container_port: String(p.container_port),
          kind: p.kind,
          host_port: "",
        })),
        env: [],
        volumes: [],
      })
      prefilled.current = prefillKey
      return
    }
    if (discovered.isPending) return
    const rows = (discovered.data?.ports ?? []).map((p) => ({
      container_port: String(p.container_port),
      kind: defaultKind(p, appsDomain !== null),
      host_port: "",
    }))
    form.reset({ name: "", ports: rows, env: [], volumes: [] })
    prefilled.current = prefillKey
  }, [prefillKey, target, discovered.isPending, discovered.data, appsDomain, isDirty, form])

  if (!target) return null
  const pending = create.isPending || replace.isPending
  // Per-server subdomain label for the hostname preview (mirror of the
  // controller's dns_label) — apps live at <name>.<server>.<domain>.
  const serverSlug =
    target.slot.serverName
      .toLowerCase()
      .replace(/_/g, "-")
      .replace(/[^a-z0-9-]/g, "")
      .replace(/^-+|-+$/g, "") || "<server>"

  const onSubmit = form.handleSubmit((values) => {
    const req = {
      slot_id: target.slot.slotId,
      registry_tag_id: target.tag.registryTagId,
      name: values.name.trim() || undefined,
      ports: values.ports.map((p) => ({
        container_port: Number(p.container_port),
        kind: p.kind,
        // Web kinds are proxy-published — a pinned host port (possibly
        // left over from a kind switch) must not reach the API.
        host_port:
          p.kind === "HTTP" || p.kind === "HTTPS" || p.host_port === ""
            ? null
            : Number(p.host_port),
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
            hint={[
              target.replaces
                ? "Prefilled from the deployment being replaced — keeping the name keeps the app URL."
                : discovered.data?.ports.length
                  ? `Prefilled from the image's EXPOSE list (${discovered.data.ports.length}).`
                  : discovered.isSuccess
                    ? "The image declares no EXPOSE ports — add the ports your app listens on manually."
                    : "TCP/UDP map directly onto the server IP.",
              appsDomain
                ? `HTTP/S ports publish at https://<name>.${serverSlug}.${appsDomain}.`
                : "HTTP/S publishing is disabled (no apps domain configured).",
            ].join(" ")}
            onAdd={() => ports.append({ container_port: "8080", kind: "TCP", host_port: "" })}
          />
          {ports.fields.map((field, i) => {
            const kind = form.watch(`ports.${i}.kind`)
            const isWeb = kind === "HTTP" || kind === "HTTPS"
            // Mirror of the controller's hostname rule:
            // <name>.<server>.<domain>, or <name>-<port>.<server>.<domain>
            // with several web ports.
            const slug =
              form
                .watch("name")
                .trim()
                .toLowerCase()
                .replace(/_/g, "-")
                .replace(/^-+|-+$/g, "") || "<name>"
            const webCount = form
              .watch("ports")
              .filter((p) => p.kind === "HTTP" || p.kind === "HTTPS").length
            const previewHost = `${slug}${
              webCount > 1 ? `-${form.watch(`ports.${i}.container_port`)}` : ""
            }.${serverSlug}.${appsDomain}`
            return (
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
                <Field className="w-28">
                  <FieldLabel>Kind</FieldLabel>
                  <Select
                    value={kind}
                    onValueChange={(v) => {
                      form.setValue(`ports.${i}.kind`, v as PortKind)
                      // Web kinds hide the host field; a stale invalid
                      // value would fail validation invisibly (review
                      // finding) — clear it on the way in.
                      if (v === "HTTP" || v === "HTTPS") {
                        form.setValue(`ports.${i}.host_port`, "")
                      }
                    }}
                  >
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {appsDomain ? (
                        <>
                          <SelectItem value="HTTP">HTTP</SelectItem>
                          <SelectItem value="HTTPS">HTTPS</SelectItem>
                        </>
                      ) : null}
                      <SelectItem value="TCP">TCP</SelectItem>
                      <SelectItem value="UDP">UDP</SelectItem>
                    </SelectContent>
                  </Select>
                </Field>
                <Field className="flex-1">
                  <FieldLabel htmlFor={`port-h-${i}`}>
                    {isWeb ? "Published" : "Host (optional)"}
                  </FieldLabel>
                  {isWeb ? (
                    <p
                      className="flex h-9 max-w-44 items-center truncate font-mono text-xs text-muted-foreground"
                      title={`https://${previewHost}`}
                    >
                      {previewHost}
                    </p>
                  ) : (
                    <Input
                      id={`port-h-${i}`}
                      inputMode="numeric"
                      placeholder="auto"
                      {...form.register(`ports.${i}.host_port`)}
                    />
                  )}
                  {!isWeb && form.formState.errors.ports?.[i]?.host_port ? (
                    <FieldError>20000–29999</FieldError>
                  ) : null}
                </Field>
                <RemoveRowButton onClick={() => ports.remove(i)} />
              </div>
            )
          })}

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
