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
export const SLOT_STATE_META: Record<SlotState, StateMeta> = {
  FREE: { label: "Free", dotClass: "bg-slot-free", textClass: "text-slot-free" },
  RESERVED: { label: "Reserved", dotClass: "bg-slot-reserved", textClass: "text-slot-reserved" },
  DEPLOYING: { label: "Deploying", dotClass: "bg-slot-reserved", textClass: "text-slot-reserved" },
  RUNNING: { label: "Running", dotClass: "bg-slot-running", textClass: "text-slot-running" },
  FAILED: { label: "Failed", dotClass: "bg-slot-failed", textClass: "text-slot-failed" },
  STOPPING: { label: "Stopping", dotClass: "bg-slot-reserved", textClass: "text-slot-reserved" },
  OFFLINE: { label: "Offline", dotClass: "bg-slot-offline", textClass: "text-slot-offline" },
}

/** The four states shown in the dashboard legend (per the mockup). */
export const LEGEND_STATES: readonly SlotState[] = ["FREE", "RUNNING", "RESERVED", "OFFLINE"]
