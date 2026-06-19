import { describe, it, expect } from "vitest"
import {
  SLOT_STATES,
  DEPLOYMENT_STATES,
  SLOT_STATE_META,
  DEPLOYMENT_STATE_META,
  SERVER_STATUS_META,
  LEGEND_STATES,
} from "@/lib/states"

describe("slot state metadata", () => {
  it("covers every slot state", () => {
    const missing = SLOT_STATES.filter((s) => !SLOT_STATE_META[s])
    expect(missing).toEqual([])
  })

  it("uses only semantic color tokens (Project Invariant: no raw hex)", () => {
    for (const s of SLOT_STATES) {
      const meta = SLOT_STATE_META[s]
      expect(meta.label.length).toBeGreaterThan(0)
      expect(meta.dotClass).toMatch(/^bg-slot-/)
      expect(meta.textClass).toMatch(/^text-slot-/)
    }
  })

  it("distinguishes a live slot from in-flight / broken ones", () => {
    // Running must not share its dot token with Freeing or Failed, or the
    // operator can't tell a live slot from one in transition / errored.
    expect(SLOT_STATE_META.RUNNING.dotClass).not.toBe(SLOT_STATE_META.STOPPING.dotClass)
    expect(SLOT_STATE_META.RUNNING.dotClass).not.toBe(SLOT_STATE_META.FAILED.dotClass)
    expect(SLOT_STATE_META.FREE.dotClass).not.toBe(SLOT_STATE_META.FAILED.dotClass)
  })

  it("renders every legend state from the slot map", () => {
    for (const s of LEGEND_STATES) expect(SLOT_STATE_META[s]).toBeDefined()
  })
})

describe("deployment + server status metadata", () => {
  it("covers every deployment state with semantic tokens", () => {
    const missing = DEPLOYMENT_STATES.filter((s) => !DEPLOYMENT_STATE_META[s])
    expect(missing).toEqual([])
    for (const s of DEPLOYMENT_STATES) {
      expect(DEPLOYMENT_STATE_META[s].dotClass).toMatch(/^bg-slot-/)
      expect(DEPLOYMENT_STATE_META[s].textClass).toMatch(/^text-slot-/)
    }
  })

  it("maps all three server statuses", () => {
    for (const s of ["ONLINE", "OFFLINE", "DEGRADED"] as const) {
      expect(SERVER_STATUS_META[s]).toBeDefined()
    }
  })
})
