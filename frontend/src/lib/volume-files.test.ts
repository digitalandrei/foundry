import { describe, expect, it } from "vitest"

import {
  agentSupportsVolumeFiles,
  joinVolumePath,
  parentVolumePath,
} from "@/lib/volume-files"

describe("volume file helpers", () => {
  it("keeps paths relative to their selected volume", () => {
    expect(joinVolumePath("", "models")).toBe("models")
    expect(joinVolumePath("models/checkpoints", "flux.safetensors")).toBe(
      "models/checkpoints/flux.safetensors",
    )
    expect(parentVolumePath("models/checkpoints")).toBe("models")
    expect(parentVolumePath("models")).toBe("")
  })

  it("gates the UI on the agent protocol version", () => {
    expect(agentSupportsVolumeFiles("0.55.9")).toBe(false)
    expect(agentSupportsVolumeFiles("0.56.0")).toBe(true)
    expect(agentSupportsVolumeFiles("v1.0.0-dev")).toBe(true)
    expect(agentSupportsVolumeFiles(null)).toBe(false)
  })
})
