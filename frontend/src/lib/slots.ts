// Slot occupancy + deploy-eligibility — the single source shared by the
// dashboard grid (drop targets) and the tap-to-deploy slot picker, so the
// two can never disagree about what a slot is or whether it can take a
// container. Mirrors docs/UI-DESIGN.md § Drag interaction.

import { SLOT_STATE_META } from "@/lib/states"
import type { DeploymentSummary, ServerSummary, SlotSummary } from "@/lib/types"

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

/** Latest occupying deployment per slot id. */
export function occupantsBySlot(
  deployments: readonly DeploymentSummary[] | undefined,
): Map<string, DeploymentSummary> {
  const occupants = new Map<string, DeploymentSummary>()
  for (const d of deployments ?? []) {
    if (!OCCUPYING_STATES.has(d.state)) continue
    const existing = occupants.get(d.slot_id)
    if (!existing || d.created_at > existing.created_at) occupants.set(d.slot_id, d)
  }
  return occupants
}

export interface SlotDeployability {
  /** Can a container be placed here right now? */
  deployable: boolean
  /** When deployable, true if placing replaces a running deployment (vs.
   * a fresh deploy onto a free slot). */
  replace: boolean
  /** Why placing is blocked — a short human phrase for tooltips/hints;
   * null when deployable. */
  reason: string | null
}

/** The deploy rule in one place: FREE → deploy, RUNNING → replace,
 * anything else is inactive; the server must be ONLINE with Docker up,
 * and no non-Foundry container may be holding the device. */
export function slotDeployability(
  slot: SlotSummary,
  server: ServerSummary,
  occupant: DeploymentSummary | undefined,
): SlotDeployability {
  // A FREE slot shows nothing even if a now-dismissable deployment still
  // references it; only then does an external (non-Foundry) container on
  // the device count against us.
  const occupied = slot.state !== "FREE" && occupant !== undefined
  const external = occupied ? null : slot.external

  if (server.status !== "ONLINE") return { deployable: false, replace: false, reason: "server offline" }
  if (server.docker_ok === false)
    return { deployable: false, replace: false, reason: "Docker stopped — deploys blocked" }
  if (external?.running)
    return { deployable: false, replace: false, reason: "GPU held by a non-Foundry container" }
  if (slot.state === "FREE") return { deployable: true, replace: false, reason: null }
  if (slot.state === "RUNNING") return { deployable: true, replace: true, reason: null }
  return { deployable: false, replace: false, reason: SLOT_STATE_META[slot.state].label.toLowerCase() }
}
