import { useState } from "react"
import { useForm } from "react-hook-form"
import { zodResolver } from "@hookform/resolvers/zod"
import { Link } from "@tanstack/react-router"
import { PencilIcon, Trash2Icon } from "lucide-react"
import { z } from "zod"

import {
  useCreateInstance,
  useDeleteInstance,
  useInstancesFull,
  useUpdateInstance,
} from "@/hooks/use-instances"
import type { InstanceAdmin as InstanceAdminType } from "@/lib/types"
import { useConfirm } from "@/components/confirm-dialog"
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
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Separator } from "@/components/ui/separator"
import { Skeleton } from "@/components/ui/skeleton"

const url = z
  .string()
  .trim()
  .url("must be a valid URL")
  .refine((u) => u.startsWith("https://") || u.startsWith("http://localhost"), {
    message: "must be https://",
  })

const schema = z.object({
  name: z.string().trim().min(1, "required").max(100),
  base_url: url,
  registry_url: url,
  oauth_client_id: z.string().trim().min(1, "required"),
  oauth_client_secret: z.string().trim().min(1, "required"),
})
type FormValues = z.infer<typeof schema>

/** Admin-only GitLab instance onboarding (Settings). Mirrors the
 * `foundry-controller instance add` bootstrap CLI; instances can also
 * be edited (URLs, secret rotation, enable/disable) and removed. */
export function InstanceAdmin() {
  const confirm = useConfirm()
  const instances = useInstancesFull(true)
  const create = useCreateInstance()
  const remove = useDeleteInstance()
  const [editing, setEditing] = useState<InstanceAdminType | null>(null)
  const redirectUri = `${window.location.origin}/auth/callback`

  const form = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: {
      name: "",
      base_url: "",
      registry_url: "",
      oauth_client_id: "",
      oauth_client_secret: "",
    },
  })

  const onSubmit = form.handleSubmit((values) =>
    create.mutate(values, { onSuccess: () => form.reset() }),
  )

  return (
    <div className="flex flex-col gap-4">
      {instances.isPending ? (
        <Skeleton className="h-10 w-full" />
      ) : instances.isError ? (
        <p className="text-sm text-muted-foreground">Could not load instances.</p>
      ) : instances.data.length === 0 ? (
        <p className="text-sm text-muted-foreground">No instances onboarded yet.</p>
      ) : (
        <ul className="flex flex-col gap-2">
          {instances.data.map((i) => (
            <li key={i.id} className="flex items-center gap-2 rounded-md border p-2 text-sm">
              <span className="font-medium">{i.name}</span>
              <span className="truncate text-muted-foreground">{i.base_url}</span>
              <Badge variant={i.enabled ? "secondary" : "outline"} className="ml-auto">
                {i.enabled ? "enabled" : "disabled"}
              </Badge>
              <Button
                variant="ghost"
                size="icon"
                className="size-8"
                aria-label={`Edit ${i.name}`}
                title="Edit"
                onClick={() => setEditing(i)}
              >
                <PencilIcon className="size-3.5" />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                className="size-8 text-slot-failed"
                aria-label={`Remove ${i.name}`}
                title="Remove (disable instead if it has history)"
                disabled={remove.isPending}
                onClick={async () => {
                  if (
                    await confirm({
                      title: `Remove "${i.name}"?`,
                      description: "This GitLab instance will be removed. This cannot be undone.",
                      destructive: true,
                    })
                  ) {
                    remove.mutate(i.id)
                  }
                }}
              >
                <Trash2Icon className="size-3.5" />
              </Button>
            </li>
          ))}
        </ul>
      )}

      <InstanceEditDialog instance={editing} onClose={() => setEditing(null)} />

      <Separator />

      <div className="rounded-md bg-muted p-3 text-xs text-muted-foreground">
        <p className="mb-1 font-medium text-foreground">
          Create an OAuth application on the GitLab instance first
        </p>
        <p>
          GitLab → Admin Area, Group Settings, or Profile → Applications (“Confidential” on). Then
          copy the Application ID and Secret below.
        </p>
        <p className="mt-1">
          Redirect URI: <code className="font-mono text-foreground">{redirectUri}</code>
        </p>
        <p>
          Scopes:{" "}
          <code className="font-mono text-foreground">
            openid profile email read_api read_registry
          </code>
        </p>
        <p className="mt-1">
          <Link to="/help/gitlab-oauth" className="underline underline-offset-2 hover:text-foreground">
            Full setup guide — which permissions and why →
          </Link>
        </p>
      </div>

      <form onSubmit={onSubmit} noValidate>
        <FieldGroup className="gap-4">
          <Field data-invalid={!!form.formState.errors.name}>
            <FieldLabel htmlFor="inst-name">Display name</FieldLabel>
            <Input id="inst-name" placeholder="Company GitLab" {...form.register("name")} />
            {form.formState.errors.name ? (
              <FieldError>{form.formState.errors.name.message}</FieldError>
            ) : null}
          </Field>
          <Field data-invalid={!!form.formState.errors.base_url}>
            <FieldLabel htmlFor="inst-base">Base URL</FieldLabel>
            <Input
              id="inst-base"
              placeholder="https://gitlab.example.com"
              {...form.register("base_url")}
            />
            {form.formState.errors.base_url ? (
              <FieldError>{form.formState.errors.base_url.message}</FieldError>
            ) : null}
          </Field>
          <Field data-invalid={!!form.formState.errors.registry_url}>
            <FieldLabel htmlFor="inst-registry">Registry URL</FieldLabel>
            <Input
              id="inst-registry"
              placeholder="https://registry.example.com"
              {...form.register("registry_url")}
            />
            <FieldDescription>
              The container registry host of the instance (often{" "}
              <code className="font-mono">registry.&lt;domain&gt;</code>).
            </FieldDescription>
            {form.formState.errors.registry_url ? (
              <FieldError>{form.formState.errors.registry_url.message}</FieldError>
            ) : null}
          </Field>
          <Field data-invalid={!!form.formState.errors.oauth_client_id}>
            <FieldLabel htmlFor="inst-cid">OAuth Application ID</FieldLabel>
            <Input id="inst-cid" autoComplete="off" {...form.register("oauth_client_id")} />
            {form.formState.errors.oauth_client_id ? (
              <FieldError>{form.formState.errors.oauth_client_id.message}</FieldError>
            ) : null}
          </Field>
          <Field data-invalid={!!form.formState.errors.oauth_client_secret}>
            <FieldLabel htmlFor="inst-secret">OAuth Application Secret</FieldLabel>
            <Input
              id="inst-secret"
              type="password"
              autoComplete="off"
              {...form.register("oauth_client_secret")}
            />
            <FieldDescription>Encrypted at rest; never shown again.</FieldDescription>
            {form.formState.errors.oauth_client_secret ? (
              <FieldError>{form.formState.errors.oauth_client_secret.message}</FieldError>
            ) : null}
          </Field>
          <Button type="submit" disabled={create.isPending} className="self-start">
            {create.isPending ? "Onboarding…" : "Onboard instance"}
          </Button>
        </FieldGroup>
      </form>
    </div>
  )
}

const editSchema = z.object({
  name: z.string().trim().min(1, "required").max(100),
  base_url: url,
  registry_url: url,
  oauth_client_id: z.string().trim().min(1, "required"),
  // Optional on edit — empty keeps the stored secret.
  oauth_client_secret: z.string(),
  enabled: z.boolean(),
})
type EditValues = z.infer<typeof editSchema>

/** Edit an onboarded instance: URLs, client id, optional secret
 * rotation, enable/disable. The secret is write-only (never returned),
 * so an empty field keeps the existing one. */
function InstanceEditDialog({
  instance,
  onClose,
}: {
  instance: InstanceAdminType | null
  onClose: () => void
}) {
  const update = useUpdateInstance()
  const form = useForm<EditValues>({
    resolver: zodResolver(editSchema),
    // Remount per instance so defaults reflect the selected row.
    values: instance
      ? {
          name: instance.name,
          base_url: instance.base_url,
          registry_url: instance.registry_url,
          oauth_client_id: instance.oauth_client_id,
          oauth_client_secret: "",
          enabled: instance.enabled,
        }
      : undefined,
  })

  if (!instance) return null

  const onSubmit = form.handleSubmit((values) => {
    update.mutate(
      {
        id: instance.id,
        req: {
          name: values.name,
          base_url: values.base_url,
          registry_url: values.registry_url,
          oauth_client_id: values.oauth_client_id,
          oauth_client_secret: values.oauth_client_secret.trim() || undefined,
          enabled: values.enabled,
        },
      },
      { onSuccess: onClose },
    )
  })

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Edit {instance.name}</DialogTitle>
          <DialogDescription>
            URLs and OAuth settings. Disabling hides it from login without losing history.
          </DialogDescription>
        </DialogHeader>
        <form onSubmit={onSubmit} noValidate>
          <FieldGroup className="gap-4">
            <Field data-invalid={!!form.formState.errors.name}>
              <FieldLabel htmlFor="edit-name">Display name</FieldLabel>
              <Input id="edit-name" {...form.register("name")} />
              {form.formState.errors.name ? (
                <FieldError>{form.formState.errors.name.message}</FieldError>
              ) : null}
            </Field>
            <Field data-invalid={!!form.formState.errors.base_url}>
              <FieldLabel htmlFor="edit-base">Base URL</FieldLabel>
              <Input id="edit-base" {...form.register("base_url")} />
              {form.formState.errors.base_url ? (
                <FieldError>{form.formState.errors.base_url.message}</FieldError>
              ) : null}
            </Field>
            <Field data-invalid={!!form.formState.errors.registry_url}>
              <FieldLabel htmlFor="edit-registry">Registry URL</FieldLabel>
              <Input id="edit-registry" {...form.register("registry_url")} />
              {form.formState.errors.registry_url ? (
                <FieldError>{form.formState.errors.registry_url.message}</FieldError>
              ) : null}
            </Field>
            <Field data-invalid={!!form.formState.errors.oauth_client_id}>
              <FieldLabel htmlFor="edit-cid">OAuth Application ID</FieldLabel>
              <Input id="edit-cid" autoComplete="off" {...form.register("oauth_client_id")} />
              {form.formState.errors.oauth_client_id ? (
                <FieldError>{form.formState.errors.oauth_client_id.message}</FieldError>
              ) : null}
            </Field>
            <Field>
              <FieldLabel htmlFor="edit-secret">OAuth Application Secret</FieldLabel>
              <Input
                id="edit-secret"
                type="password"
                autoComplete="off"
                placeholder="•••••••• (unchanged)"
                {...form.register("oauth_client_secret")}
              />
              <FieldDescription>Leave blank to keep the current secret.</FieldDescription>
            </Field>
            <label className="flex items-center gap-2 text-sm">
              <Checkbox
                checked={form.watch("enabled")}
                onCheckedChange={(v) => form.setValue("enabled", v === true)}
              />
              Enabled (available for login and deployments)
            </label>
          </FieldGroup>
          <DialogFooter className="mt-4">
            <Button type="button" variant="outline" onClick={onClose} disabled={update.isPending}>
              Cancel
            </Button>
            <Button type="submit" disabled={update.isPending}>
              {update.isPending ? "Saving…" : "Save changes"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
