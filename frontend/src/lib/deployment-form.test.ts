import { describe, expect, it } from "vitest"

import {
  defaultPortKind,
  deploymentFormSchema,
  MEM_UNLIMITED_GB,
} from "@/lib/deployment-form"

describe("deployment volume policy", () => {
  it("accepts a project/server mount with purge-on-redeploy", () => {
    const parsed = deploymentFormSchema.safeParse({
      name: "",
      ports: [],
      env: [],
      mem_limit_gb: MEM_UNLIMITED_GB,
      volumes: [
        {
          volume_id: null,
          volume_name: "models",
          container_path: "/models",
          read_only: false,
          visibility: "PROJECT",
          placement: "SERVER",
          purge_on_redeploy: true,
        },
      ],
    })
    expect(parsed.success).toBe(true)
  })

  it("rejects unknown visibility and placement strings", () => {
    const parsed = deploymentFormSchema.safeParse({
      name: "",
      ports: [],
      env: [],
      mem_limit_gb: MEM_UNLIMITED_GB,
      volumes: [
        {
          volume_name: "models",
          container_path: "/models",
          read_only: false,
          visibility: "EVERYONE",
          placement: "CLUSTER",
          purge_on_redeploy: false,
        },
      ],
    })
    expect(parsed.success).toBe(false)
  })

  it("recognizes ComfyUI's exposed 8188 as an HTTP app", () => {
    expect(defaultPortKind({ container_port: 8188, protocol: "tcp" }, true)).toBe("HTTP")
    expect(defaultPortKind({ container_port: 8188, protocol: "tcp" }, false)).toBe("TCP")
  })
})
