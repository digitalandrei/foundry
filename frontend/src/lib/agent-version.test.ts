import { describe, expect, it } from "vitest"

import { agentSupportsOperationalSafety, agentVersionAtLeast } from "@/lib/agent-version"

describe("agent version gates", () => {
  it("accepts release and prefixed versions at or above the minimum", () => {
    expect(agentSupportsOperationalSafety("0.59.0")).toBe(true)
    expect(agentSupportsOperationalSafety("v0.60.0-dev")).toBe(true)
    expect(agentSupportsOperationalSafety("0.58.9")).toBe(false)
  })

  it("rejects absent and malformed versions", () => {
    expect(agentVersionAtLeast(null, [0, 59, 0])).toBe(false)
    expect(agentVersionAtLeast("0.59", [0, 59, 0])).toBe(false)
  })
})
