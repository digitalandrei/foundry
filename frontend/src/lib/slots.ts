// Slot occupancy + deploy-eligibility — the single source shared by the
// dashboard grid (drop targets) and the tap-to-deploy slot picker, so the
// two can never disagree about what a slot is or whether it can take a
// container. Mirrors docs/UI-DESIGN.md § Drag interaction.

import { SLOT_STATE_META } from "@/lib/states"
import type { DeploymentSummary, GpuGroup, GpuSummary, ServerSummary, SlotSummary } from "@/lib/types"

/** Deployment states that still occupy a slot (the container is on the
 * host). REMOVED/REPLACED have left, so they hold nothing. */
const OCCUPYING_STATES = new Set<DeploymentSummary["state"]>([
  "PENDING",
  "VALIDATING",
  "PULLING_IMAGE",
  "CREATING_CONTAINER",
  "STARTING",
  "RUNNING",
  "STOPPING",
  "STOPPED",
  "RESTARTING",
  "REMOVING",
  "FAILED",
])

/** Occupying deployments per slot id, newest-first. A multi-use slot can
 * have several. A *group* deploy is independent of its members' own slots —
 * it occupies the group (see `occupantsByGroup`), not the member GPUs, so
 * the member cards stay free for individual deploys (the operator owns any
 * over-subscription). It is therefore never folded onto member slots here. */
export function occupantsBySlot(
  deployments: readonly DeploymentSummary[] | undefined,
): Map<string, DeploymentSummary[]> {
  const occupants = new Map<string, DeploymentSummary[]>()
  for (const d of deployments ?? []) {
    if (!OCCUPYING_STATES.has(d.state)) continue
    // Group deploys belong to the group slot only, never the member cards.
    if (d.gpu_group_id) continue
    const ids = d.slot_ids.length > 0 ? d.slot_ids : [d.slot_id]
    for (const id of ids) {
      const list = occupants.get(id)
      if (list) list.push(d)
      else occupants.set(id, [d])
    }
  }
  for (const list of occupants.values()) {
    list.sort((a, b) => (a.created_at < b.created_at ? 1 : a.created_at > b.created_at ? -1 : 0))
  }
  return occupants
}

/** One deploy position within a GPU. A slot exposes `max_occupants`
 * positions (1 for single-use, up to 4 for multi-use); positions are
 * numbered 1-based across the GPU so the grid/picker can render exactly as
 * many "SLOT n" chips as the operator configured. */
export interface SlotPosition {
  slot: SlotSummary
  /** Label within the GPU, from the slot's name: the card index for a
   * full-GPU slot (`0`), `<card>.<slice>` for a MIG instance (`3.1`). A
   * multi-use slot's extra positions get a ` ·k` co-tenant suffix. */
  label: string
  /** The occupant filling this position, if any. */
  occupant: DeploymentSummary | undefined
  /** All active occupants on the underlying slot (drives deployability). */
  occupants: DeploymentSummary[]
  /** First position of its slot — where an external holder is surfaced. */
  firstOfSlot: boolean
}

/** Expand a GPU's slots into per-position entries (see `SlotPosition`). The
 * label is the slot's own name (card index, or `<card>.<slice>` for MIG); a
 * multi-use slot exposes `max_occupants` positions, the extras suffixed ` ·k`
 * to stay distinct. The i-th occupant fills the i-th position; the rest are
 * free capacity. */
export function gpuSlotPositions(
  gpu: GpuSummary,
  occupantsBySlot: Map<string, DeploymentSummary[]>,
): SlotPosition[] {
  const out: SlotPosition[] = []
  for (const slot of gpu.slots) {
    const occupants = occupantsBySlot.get(slot.id) ?? []
    const positions = Math.max(1, slot.max_occupants)
    for (let i = 0; i < positions; i++) {
      const label = positions === 1 ? slot.name : `${slot.name} ·${i + 1}`
      out.push({ slot, label, occupant: occupants[i], occupants, firstOfSlot: i === 0 })
    }
  }
  return out
}

/** Occupying deployments per group id, newest-first — the group's *own*
 * occupancy (one container across every member GPU), distinct from the
 * member-slot folding in `occupantsBySlot`. A multi-use group can hold
 * several; single-use holds at most one. */
export function occupantsByGroup(
  deployments: readonly DeploymentSummary[] | undefined,
): Map<string, DeploymentSummary[]> {
  const occupants = new Map<string, DeploymentSummary[]>()
  for (const d of deployments ?? []) {
    if (!OCCUPYING_STATES.has(d.state) || !d.gpu_group_id) continue
    const list = occupants.get(d.gpu_group_id)
    if (list) list.push(d)
    else occupants.set(d.gpu_group_id, [d])
  }
  for (const list of occupants.values()) {
    list.sort((a, b) => (a.created_at < b.created_at ? 1 : a.created_at > b.created_at ? -1 : 0))
  }
  return occupants
}

/** One deploy position within a group ("SLOT n"). A group exposes
 * `max_occupants` positions (1 for single-use, up to 4 for multi-use),
 * rendered exactly like a GPU's slots so the operator deploys onto a group
 * the same way they deploy onto a GPU. */
export interface GroupPosition {
  /** 1-based label within the group (`SLOT 1`, `SLOT 2`, …). */
  label: number
  /** The group deploy filling this position, if any. */
  occupant: DeploymentSummary | undefined
}

/** Expand a group into per-position entries, the i-th occupant filling the
 * i-th position and the rest free capacity — the group-level twin of
 * `gpuSlotPositions`. */
export function groupSlotPositions(
  group: GpuGroup,
  occupantsByGroup: Map<string, DeploymentSummary[]>,
): GroupPosition[] {
  const occupants = occupantsByGroup.get(group.id) ?? []
  const positions = Math.max(1, group.max_occupants)
  const out: GroupPosition[] = []
  for (let i = 0; i < positions; i++) out.push({ label: i + 1, occupant: occupants[i] })
  return out
}

export interface SlotDeployability {
  /** Can a container be placed here right now? */
  deployable: boolean
  /** When deployable, true if placing replaces a running deployment (vs.
   * a fresh deploy onto a free slot). Never set for multi-use slots —
   * they take a new co-tenant rather than replacing one. */
  replace: boolean
  /** Why placing is blocked — a short human phrase for tooltips/hints;
   * null when deployable. */
  reason: string | null
}

/** The deploy rule in one place. The server must be ONLINE with Docker up,
 * and no non-Foundry container may be holding the device. Then:
 * - single-use (`max_occupants <= 1`): FREE → deploy, RUNNING → replace,
 *   anything else is inactive (preserves the original behaviour).
 * - multi-use (`max_occupants > 1`): deployable while the live occupant
 *   count is below the cap (never "replace"); `full · k/N` when at cap;
 *   inactive while OFFLINE. */
export function slotDeployability(
  slot: SlotSummary,
  server: ServerSummary,
  occupants: DeploymentSummary[],
): SlotDeployability {
  // An external (non-Foundry) container on the device only counts against
  // us when Foundry itself is holding nothing here.
  const external = occupants.length === 0 ? slot.external : null

  if (server.status !== "ONLINE") return { deployable: false, replace: false, reason: "server offline" }
  if (server.docker_ok === false)
    return { deployable: false, replace: false, reason: "Docker stopped — deploys blocked" }
  const dockerGpu = server.readiness?.checks.find((check) => check.code === "docker_gpu")
  if (server.readiness && dockerGpu?.status !== "READY")
    return {
      deployable: false,
      replace: false,
      reason: dockerGpu?.detail ?? "NVIDIA container support has not been verified",
    }
  if (external?.running)
    return { deployable: false, replace: false, reason: "GPU held by a non-Foundry container" }

  if (slot.max_occupants > 1) {
    // Multi-use: soft sharing up to the cap. OFFLINE is never deployable;
    // otherwise it takes another co-tenant until full.
    if (slot.state === "OFFLINE")
      return { deployable: false, replace: false, reason: SLOT_STATE_META.OFFLINE.label.toLowerCase() }
    if (occupants.length < slot.max_occupants)
      return { deployable: true, replace: false, reason: null }
    return { deployable: false, replace: false, reason: `full · ${occupants.length}/${slot.max_occupants}` }
  }

  // Single-use: the original binary rule, keyed off the slot state.
  if (slot.state === "FREE") return { deployable: true, replace: false, reason: null }
  if (slot.state === "RUNNING") return { deployable: true, replace: true, reason: null }
  return { deployable: false, replace: false, reason: SLOT_STATE_META[slot.state].label.toLowerCase() }
}
