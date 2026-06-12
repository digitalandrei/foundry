// Mirror of the Rust wire contract in shared/src/states.rs — the string
// forms are identical to what the API serves and the DB stores. If a
// variant changes there, it changes here in the same commit.

export const SLOT_STATES = [
  "FREE",
  "RESERVED",
  "DEPLOYING",
  "RUNNING",
  "FAILED",
  "STOPPING",
  "OFFLINE",
] as const
export type SlotState = (typeof SLOT_STATES)[number]

export const DEPLOYMENT_STATES = [
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
  "REMOVED",
  "FAILED",
  "REPLACED",
] as const
export type DeploymentState = (typeof DEPLOYMENT_STATES)[number]

interface StateMeta {
  label: string
  /** Background token class for chips/dots (docs/UI-DESIGN.md colors). */
  dotClass: string
  /** Foreground token class for status text. */
  textClass: string
}

// The single state→color map (docs/FRONTEND_RULES.md § Structure &
// Reuse). Every chip, dot, badge, and status cell goes through this.
// Labels follow the operator's lifecycle vocabulary: a slot is "Locked"
// the moment a deploy claims it, "Deploying" while the agent works,
// "Running" when live, "Freeing" while a stop/remove is in flight.
export const SLOT_STATE_META: Record<SlotState, StateMeta> = {
  FREE: { label: "Free", dotClass: "bg-slot-free", textClass: "text-slot-free" },
  RESERVED: { label: "Locked", dotClass: "bg-slot-reserved", textClass: "text-slot-reserved" },
  DEPLOYING: { label: "Deploying", dotClass: "bg-slot-reserved", textClass: "text-slot-reserved" },
  RUNNING: { label: "Running", dotClass: "bg-slot-running", textClass: "text-slot-running" },
  FAILED: { label: "Failed", dotClass: "bg-slot-failed", textClass: "text-slot-failed" },
  STOPPING: { label: "Freeing", dotClass: "bg-slot-reserved", textClass: "text-slot-reserved" },
  OFFLINE: { label: "Offline", dotClass: "bg-slot-offline", textClass: "text-slot-offline" },
}

/** The four states shown in the dashboard legend (per the mockup). */
export const LEGEND_STATES: readonly SlotState[] = ["FREE", "RUNNING", "RESERVED", "OFFLINE"]

import type { ServerStatus } from "@/lib/types"

/** Server liveness colors — same tokens, same single-source rule. */
export const SERVER_STATUS_META: Record<ServerStatus, StateMeta> = {
  ONLINE: { label: "Online", dotClass: "bg-slot-free", textClass: "text-slot-free" },
  OFFLINE: { label: "Offline", dotClass: "bg-slot-offline", textClass: "text-slot-offline" },
  DEGRADED: { label: "Degraded", dotClass: "bg-slot-reserved", textClass: "text-slot-reserved" },
}

/** Deployment lifecycle colors (mockup: Running green in tables;
 * everything transitional yellow; terminal gray; failed red). */
export const DEPLOYMENT_STATE_META: Record<DeploymentState, StateMeta> = {
  PENDING: { label: "Pending", dotClass: "bg-slot-reserved", textClass: "text-slot-reserved" },
  VALIDATING: { label: "Validating", dotClass: "bg-slot-reserved", textClass: "text-slot-reserved" },
  PULLING_IMAGE: { label: "Pulling image", dotClass: "bg-slot-reserved", textClass: "text-slot-reserved" },
  CREATING_CONTAINER: { label: "Creating", dotClass: "bg-slot-reserved", textClass: "text-slot-reserved" },
  STARTING: { label: "Starting", dotClass: "bg-slot-reserved", textClass: "text-slot-reserved" },
  RUNNING: { label: "Running", dotClass: "bg-slot-free", textClass: "text-slot-free" },
  STOPPING: { label: "Stopping", dotClass: "bg-slot-reserved", textClass: "text-slot-reserved" },
  STOPPED: { label: "Stopped", dotClass: "bg-slot-offline", textClass: "text-slot-offline" },
  RESTARTING: { label: "Restarting", dotClass: "bg-slot-reserved", textClass: "text-slot-reserved" },
  REMOVING: { label: "Removing", dotClass: "bg-slot-reserved", textClass: "text-slot-reserved" },
  REMOVED: { label: "Removed", dotClass: "bg-slot-offline", textClass: "text-slot-offline" },
  FAILED: { label: "Failed", dotClass: "bg-slot-failed", textClass: "text-slot-failed" },
  REPLACED: { label: "Replaced", dotClass: "bg-slot-offline", textClass: "text-slot-offline" },
}
