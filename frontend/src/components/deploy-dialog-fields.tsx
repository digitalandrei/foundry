import { PlusIcon, Trash2Icon } from "lucide-react"
import { useWatch, type UseFieldArrayReturn, type UseFormReturn } from "react-hook-form"

import { Button } from "@/components/ui/button"
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
import { Separator } from "@/components/ui/separator"
import { Slider } from "@/components/ui/slider"
import {
  MEM_MAX_GB,
  MEM_MIN_GB,
  MEM_PRESET_GB,
  MEM_STEP_GB,
  MEM_UNLIMITED_GB,
  type DeploymentFormValues,
} from "@/lib/deployment-form"
import type { ExposedPort, PortKind } from "@/lib/types"

type FieldsProps = {
  form: UseFormReturn<DeploymentFormValues>
  ports: UseFieldArrayReturn<DeploymentFormValues, "ports">
  envRows: UseFieldArrayReturn<DeploymentFormValues, "env">
  mounts: UseFieldArrayReturn<DeploymentFormValues, "volumes">
  imageName: string
  replacing: boolean
  appsDomain: string | null
  serverSlug: string
  discoveredPorts: ExposedPort[] | undefined
  discoveredVolumeCount: number
  discoverySucceeded: boolean
  volumeNames: string[]
}

export function DeployDialogFields({
  form,
  ports,
  envRows,
  mounts,
  imageName,
  replacing,
  appsDomain,
  serverSlug,
  discoveredPorts,
  discoveredVolumeCount,
  discoverySucceeded,
  volumeNames,
}: FieldsProps) {
  const watched = useWatch({ control: form.control })
  const memGb = watched.mem_limit_gb ?? MEM_UNLIMITED_GB
  const memUnlimited = memGb >= MEM_UNLIMITED_GB

  return (
    <>
      <Field data-invalid={!!form.formState.errors.name}>
        <FieldLabel htmlFor="dep-name">Name</FieldLabel>
        <Input
          id="dep-name"
          placeholder={`${imageName}-auto`}
          autoComplete="off"
          {...form.register("name")}
        />
        <FieldDescription>Optional — generated when empty.</FieldDescription>
        {form.formState.errors.name ? (
          <FieldError>{form.formState.errors.name.message}</FieldError>
        ) : null}
      </Field>

      <Separator />
      <Field>
        <div className="flex items-center justify-between">
          <FieldLabel>
            Memory limit
            <span className="ml-2 font-mono text-xs text-muted-foreground">
              {memUnlimited ? "Unlimited" : `${memGb} GB`}
            </span>
          </FieldLabel>
          <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <Checkbox
              checked={memUnlimited}
              onCheckedChange={(value) =>
                form.setValue(
                  "mem_limit_gb",
                  value === true ? MEM_UNLIMITED_GB : MEM_PRESET_GB,
                  { shouldDirty: true },
                )
              }
            />
            Unlimited
          </label>
        </div>
        <Slider
          className="py-2"
          min={MEM_MIN_GB}
          max={MEM_UNLIMITED_GB}
          step={MEM_STEP_GB}
          value={[memGb]}
          onValueChange={([value]) =>
            form.setValue("mem_limit_gb", value, { shouldDirty: true })
          }
          aria-label="Memory limit in GB"
        />
        <FieldDescription>
          Docker memory cap ({MEM_MIN_GB}–{MEM_MAX_GB} GB). Slide fully right or tick Unlimited
          for no cap.
        </FieldDescription>
      </Field>

      <Separator />
      <SectionHeader
        title="Ports"
        hint={[
          replacing
            ? "Prefilled from the deployment being replaced — keeping the name keeps the app URL."
            : discoveredPorts?.length
              ? `Prefilled from the image's EXPOSE list (${discoveredPorts.length}).`
              : discoverySucceeded
                ? "The image declares no EXPOSE ports — add the ports your app listens on manually."
                : "TCP/UDP map directly onto the server IP.",
          appsDomain
            ? `HTTP/S ports publish at https://<name>.${serverSlug}.${appsDomain}.`
            : "HTTP/S publishing is disabled (no apps domain configured).",
        ].join(" ")}
        onAdd={() => ports.append({ container_port: "8080", kind: "TCP", host_port: "" })}
      />
      {ports.fields.map((field, index) => {
        const kind = watched.ports?.[index]?.kind
        const isWeb = kind === "HTTP" || kind === "HTTPS"
        const slug =
          (watched.name ?? "")
            .trim()
            .toLowerCase()
            .replace(/_/g, "-")
            .replace(/^-+|-+$/g, "") || "<name>"
        const webCount = (watched.ports ?? []).filter(
          (port) => port.kind === "HTTP" || port.kind === "HTTPS",
        ).length
        const previewHost = `${slug}${
          webCount > 1 ? `-${watched.ports?.[index]?.container_port ?? ""}` : ""
        }.${serverSlug}.${appsDomain}`
        return (
          <div key={field.id} className="flex items-start gap-2">
            <Field className="flex-1">
              <FieldLabel htmlFor={`port-c-${index}`}>Container</FieldLabel>
              <Input
                id={`port-c-${index}`}
                inputMode="numeric"
                {...form.register(`ports.${index}.container_port`)}
              />
              {form.formState.errors.ports?.[index]?.container_port ? (
                <FieldError>invalid</FieldError>
              ) : null}
            </Field>
            <Field className="w-28">
              <FieldLabel>Kind</FieldLabel>
              <Select
                value={kind}
                onValueChange={(value) => {
                  form.setValue(`ports.${index}.kind`, value as PortKind)
                  if (value === "HTTP" || value === "HTTPS") {
                    form.setValue(`ports.${index}.host_port`, "")
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
              <FieldLabel htmlFor={`port-h-${index}`}>
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
                  id={`port-h-${index}`}
                  inputMode="numeric"
                  placeholder="auto"
                  {...form.register(`ports.${index}.host_port`)}
                />
              )}
              {!isWeb && form.formState.errors.ports?.[index]?.host_port ? (
                <FieldError>20000–29999</FieldError>
              ) : null}
            </Field>
            <RemoveRowButton onClick={() => ports.remove(index)} />
          </div>
        )
      })}

      <Separator />
      <SectionHeader
        title="Environment"
        onAdd={() => envRows.append({ key: "", value: "", is_secret: false })}
      />
      {envRows.fields.map((field, index) => (
        <div key={field.id} className="flex items-start gap-2">
          <Field className="flex-1">
            <Input placeholder="KEY" autoComplete="off" {...form.register(`env.${index}.key`)} />
            {form.formState.errors.env?.[index]?.key ? (
              <FieldError>invalid key</FieldError>
            ) : null}
          </Field>
          <Field className="flex-1">
            <Input
              placeholder="value"
              autoComplete="off"
              type={watched.env?.[index]?.is_secret ? "password" : "text"}
              {...form.register(`env.${index}.value`)}
            />
          </Field>
          <label className="flex h-9 items-center gap-1.5 text-xs text-muted-foreground">
            <Checkbox
              checked={watched.env?.[index]?.is_secret ?? false}
              onCheckedChange={(value) => form.setValue(`env.${index}.is_secret`, value === true)}
            />
            secret
          </label>
          <RemoveRowButton onClick={() => envRows.remove(index)} />
        </div>
      ))}

      <Separator />
      <SectionHeader
        title="Persistent storage"
        hint={`${discoveredVolumeCount ? `${discoveredVolumeCount} mount${discoveredVolumeCount === 1 ? "" : "s"} prefilled from the image. ` : ""}Volumes live at /storage/containers/<you>/<name>, survive container removal, and can be remounted later.${
          volumeNames.length ? ` Yours on this server: ${volumeNames.join(", ")}.` : ""
        }`}
        onAdd={() =>
          mounts.append({ volume_name: "", container_path: "/data", read_only: false })
        }
      />
      {mounts.fields.map((field, index) => (
        <div key={field.id} className="flex items-start gap-2">
          <Field className="flex-1">
            <Input
              placeholder="volume name"
              autoComplete="off"
              list="existing-volumes"
              {...form.register(`volumes.${index}.volume_name`)}
            />
            {form.formState.errors.volumes?.[index]?.volume_name ? (
              <FieldError>invalid name</FieldError>
            ) : null}
          </Field>
          <Field className="flex-1">
            <Input
              placeholder="/data"
              autoComplete="off"
              {...form.register(`volumes.${index}.container_path`)}
            />
            {form.formState.errors.volumes?.[index]?.container_path ? (
              <FieldError>absolute path</FieldError>
            ) : null}
          </Field>
          <label className="flex h-9 items-center gap-1.5 text-xs text-muted-foreground">
            <Checkbox
              checked={watched.volumes?.[index]?.read_only ?? false}
              onCheckedChange={(value) =>
                form.setValue(`volumes.${index}.read_only`, value === true)
              }
            />
            ro
          </label>
          <RemoveRowButton onClick={() => mounts.remove(index)} />
        </div>
      ))}
      <datalist id="existing-volumes">
        {volumeNames.map((name) => (
          <option key={name} value={name} />
        ))}
      </datalist>
    </>
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
