import { useState } from "react"
import { useForm } from "react-hook-form"
import { zodResolver } from "@hookform/resolvers/zod"
import { BoxesIcon, TriangleAlertIcon, Trash2Icon } from "lucide-react"
import { z } from "zod"

import {
  useCreateGroup,
  useDeleteGroup,
  useServerGroups,
  useSetGroupUseMode,
  useSetSlotUseMode,
} from "@/hooks/use-gpu-groups"
import { MAX_OCCUPANTS_MAX } from "@/lib/types"
import type { GpuGroup, GpuSummary, ServerSummary, SlotSummary } from "@/lib/types"
import { cn } from "@/lib/utils"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
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
import { Skeleton } from "@/components/ui/skeleton"

/** A GPU is group-eligible when it exposes a FULL_GPU slot and MIG is off
 * (grouping and MIG are mutually exclusive — plans/gpu-groups.md inv. 5). */
function isGroupEligible(gpu: GpuSummary): boolean {
  return !gpu.mig_enabled && gpu.slots.some((s) => s.slot_type === "FULL_GPU")
}

/** Admin-only server config: GPU groups (one container : N GPUs) and
 * per-slot use-mode (multi-use soft sharing). The caller gates on
 * `is_admin`; this renders nothing structural for non-admins. */
export function ServerGpuConfig({ server }: { server: ServerSummary }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <BoxesIcon className="size-4" aria-hidden /> GPU groups & slot sharing
          <Badge variant="secondary" className="text-[10px]">
            admin
          </Badge>
        </CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-6">
        <GroupsSection server={server} />
        <Separator />
        <SlotUseModeSection server={server} />
      </CardContent>
    </Card>
  )
}

// ── Groups ────────────────────────────────────────────────────────────

const groupSchema = z.object({
  name: z
    .string()
    .min(1, "required")
    .regex(/^[A-Za-z0-9][A-Za-z0-9 _-]*$/, "letters/digits/space/dash/underscore")
    .max(64),
})
type GroupForm = z.infer<typeof groupSchema>

function GroupsSection({ server }: { server: ServerSummary }) {
  const groups = useServerGroups(server.id)
  const create = useCreateGroup(server.id)
  // Member selection lives outside RHF — it's a set of GPU ids, validated
  // (2…all eligible) on submit rather than per-keystroke.
  const [selected, setSelected] = useState<Set<string>>(new Set())

  const form = useForm<GroupForm>({
    resolver: zodResolver(groupSchema),
    defaultValues: { name: "" },
  })

  const eligible = server.gpus.filter(isGroupEligible)
  const gpuById = new Map(server.gpus.map((g) => [g.id, g]))

  const toggle = (id: string) =>
    setSelected((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })

  const onSubmit = form.handleSubmit((values) => {
    if (selected.size < 2) {
      form.setError("name", { message: "pick at least 2 GPUs" })
      return
    }
    create.mutate(
      { name: values.name.trim(), gpu_ids: [...selected] },
      {
        onSuccess: () => {
          form.reset({ name: "" })
          setSelected(new Set())
        },
      },
    )
  })

  return (
    <div className="flex flex-col gap-3">
      <div>
        <p className="text-sm font-medium">Groups</p>
        <p className="text-xs text-muted-foreground">
          A named set of whole GPUs on this server; deploying to it runs one container across all of
          them (multi-GPU training, oversized models). Member GPUs stay individually deployable when
          no group job runs.
        </p>
      </div>

      {/* Existing groups */}
      {groups.isPending ? (
        <Skeleton className="h-12 w-full" />
      ) : groups.data && groups.data.length > 0 ? (
        <ul className="flex flex-col gap-1.5">
          {groups.data.map((group) => (
            <GroupRow key={group.id} group={group} server={server} gpuById={gpuById} />
          ))}
        </ul>
      ) : (
        <p className="text-xs text-muted-foreground">No groups defined yet.</p>
      )}

      {/* New group */}
      {eligible.length < 2 ? (
        <p className="text-xs text-muted-foreground">
          Need at least two full-GPU, MIG-disabled GPUs to form a group.
        </p>
      ) : (
        <form onSubmit={onSubmit} noValidate className="flex flex-col gap-3 rounded-md border p-3">
          <p className="text-sm font-medium">New group</p>
          <Field data-invalid={!!form.formState.errors.name}>
            <FieldLabel htmlFor="group-name">Name</FieldLabel>
            <Input id="group-name" placeholder="e.g. training-pair" autoComplete="off" {...form.register("name")} />
            {form.formState.errors.name ? (
              <FieldError>{form.formState.errors.name.message}</FieldError>
            ) : null}
          </Field>
          <Field>
            <FieldLabel>Members</FieldLabel>
            <FieldDescription>
              Pick 2 or more. GPUs already in other groups stay selectable (overlap is allowed).
            </FieldDescription>
            <div className="mt-1 grid gap-1.5 [grid-template-columns:repeat(auto-fill,minmax(13rem,1fr))]">
              {eligible.map((gpu) => (
                <label
                  key={gpu.id}
                  className={cn(
                    "flex cursor-pointer items-start gap-2 rounded-md border px-2.5 py-2 text-sm transition-colors",
                    selected.has(gpu.id) ? "border-slot-running/60 bg-accent/40" : "hover:bg-accent/20",
                  )}
                >
                  <Checkbox
                    className="mt-0.5"
                    checked={selected.has(gpu.id)}
                    onCheckedChange={() => toggle(gpu.id)}
                  />
                  <span className="flex min-w-0 flex-col">
                    <span className="font-medium">
                      GPU {gpu.index}
                      {gpu.model ? ` · ${gpu.model}` : ""}
                    </span>
                    <span className="text-xs text-muted-foreground">
                      {gpu.memory_mb != null ? `${Math.round(gpu.memory_mb / 1024)} GB` : "? GB"}
                      {gpu.groups.length > 0 ? ` · in ${gpu.groups.map((g) => g.name).join(", ")}` : ""}
                    </span>
                  </span>
                </label>
              ))}
            </div>
          </Field>
          <div className="flex items-center gap-3">
            <Button type="submit" size="sm" disabled={create.isPending}>
              {create.isPending ? "Creating…" : "Create group"}
            </Button>
            <span className="text-xs text-muted-foreground">{selected.size} selected</span>
          </div>
        </form>
      )}
    </div>
  )
}

/** One existing group: its members + a single/multi-use selector (set
 * after creation, like a slot) + delete. Multi-use shares the grouped
 * GPUs across up to N containers (soft sharing, no VRAM isolation). */
function GroupRow({
  group,
  server,
  gpuById,
}: {
  group: GpuGroup
  server: ServerSummary
  gpuById: Map<string, GpuSummary>
}) {
  const remove = useDeleteGroup(server.id)
  const setMode = useSetGroupUseMode(server.id)
  const multi = group.max_occupants > 1
  const setMax = (max: number) => setMode.mutate({ groupId: group.id, req: { max_occupants: max } })

  return (
    <li className="flex flex-wrap items-center gap-2 rounded-md border px-3 py-2 text-sm">
      <span className="font-medium">{group.name}</span>
      <span className="text-xs text-muted-foreground">
        {group.gpu_ids.length} GPUs · {Math.round(group.combined_vram_mb / 1024)} GB ·{" "}
        {group.gpu_ids
          .map((id) => {
            const gpu = gpuById.get(id)
            return gpu ? `GPU ${gpu.index}` : "?"
          })
          .join(", ")}
        {multi ? ` · ${group.occupants}/${group.max_occupants} in use` : ""}
      </span>
      {group.deployable ? (
        <span className="text-xs text-slot-free">deployable</span>
      ) : group.busy_reason ? (
        <span className="text-xs text-slot-reserved">{group.busy_reason}</span>
      ) : null}
      <div className="ml-auto flex items-center gap-2">
        <Select
          value={multi ? "multi" : "single"}
          onValueChange={(v) => setMax(v === "multi" ? Math.max(2, group.max_occupants) : 1)}
          disabled={setMode.isPending}
        >
          <SelectTrigger className="h-8 w-32 text-xs">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="single">Single-use</SelectItem>
            <SelectItem value="multi">Multi-use</SelectItem>
          </SelectContent>
        </Select>
        {multi ? (
          <Select
            value={String(group.max_occupants)}
            onValueChange={(v) => setMax(Number(v))}
            disabled={setMode.isPending}
          >
            <SelectTrigger className="h-8 w-24 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {Array.from({ length: MAX_OCCUPANTS_MAX - 1 }, (_, i) => i + 2).map((n) => (
                <SelectItem key={n} value={String(n)}>
                  max {n}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        ) : null}
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="size-8 shrink-0"
          disabled={group.occupants > 0 || remove.isPending}
          title={group.occupants > 0 ? "Stop the group's deployments first" : "Delete group"}
          onClick={() => remove.mutate(group.id)}
          aria-label={`Delete group ${group.name}`}
        >
          <Trash2Icon className="size-3.5" aria-hidden />
        </Button>
      </div>
    </li>
  )
}

// ── Slot use-mode ───────────────────────────────────────────────────────

function SlotUseModeSection({ server }: { server: ServerSummary }) {
  const slots = server.gpus.flatMap((gpu) =>
    gpu.slots.map((slot) => ({ gpu, slot })),
  )
  return (
    <div className="flex flex-col gap-3">
      <div>
        <p className="text-sm font-medium">Slot sharing (multi-use)</p>
        <p className="text-xs text-muted-foreground">
          A multi-use slot lets several containers share one GPU up to a max-occupant limit.
        </p>
      </div>
      <div className="flex items-start gap-2 rounded-md border border-slot-reserved/50 bg-slot-reserved/10 p-3 text-xs">
        <TriangleAlertIcon className="size-4 shrink-0 text-slot-reserved" aria-hidden />
        <p>
          <span className="font-medium">No VRAM isolation.</span> Co-tenants share the card&apos;s
          memory and compute — one container can OOM or starve the others. Prefer MIG when you need
          hardware isolation; multi-use is soft sharing you opt into per slot. Lowering the limit
          below the current occupant count stops new deploys but does <em>not</em> evict running
          tenants.
        </p>
      </div>
      {slots.length === 0 ? (
        <p className="text-xs text-muted-foreground">No slots reported yet.</p>
      ) : (
        <ul className="flex flex-col gap-1.5">
          {slots.map(({ gpu, slot }) => (
            <SlotUseModeRow key={slot.id} server={server} gpu={gpu} slot={slot} />
          ))}
        </ul>
      )}
    </div>
  )
}

function SlotUseModeRow({
  server,
  gpu,
  slot,
}: {
  server: ServerSummary
  gpu: GpuSummary
  slot: SlotSummary
}) {
  const setMode = useSetSlotUseMode(server.id)
  const multi = slot.max_occupants > 1

  const setMax = (max: number) =>
    setMode.mutate({ slotId: slot.id, req: { max_occupants: max } })

  return (
    <li className="flex flex-wrap items-center gap-2 rounded-md border px-3 py-2 text-sm">
      <span className="font-mono text-xs font-medium">SLOT {slot.name}</span>
      <span className="text-xs text-muted-foreground">
        GPU {gpu.index} · {slot.slot_type === "FULL_GPU" ? "full GPU" : (slot.mig_profile ?? "MIG")}
      </span>
      <div className="ml-auto flex items-center gap-2">
        <Select
          value={multi ? "multi" : "single"}
          onValueChange={(v) => setMax(v === "multi" ? Math.max(2, slot.max_occupants) : 1)}
          disabled={setMode.isPending}
        >
          <SelectTrigger className="h-8 w-32 text-xs">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="single">Single-use</SelectItem>
            <SelectItem value="multi">Multi-use</SelectItem>
          </SelectContent>
        </Select>
        {multi ? (
          <Select
            value={String(slot.max_occupants)}
            onValueChange={(v) => setMax(Number(v))}
            disabled={setMode.isPending}
          >
            <SelectTrigger className="h-8 w-24 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {Array.from(
                { length: MAX_OCCUPANTS_MAX - 1 },
                (_, i) => i + 2, // multi-use range: 2…MAX_OCCUPANTS_MAX
              ).map((n) => (
                <SelectItem key={n} value={String(n)}>
                  max {n}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        ) : null}
      </div>
    </li>
  )
}
