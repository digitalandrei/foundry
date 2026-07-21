import { describe, expect, it } from "vitest"

import {
  defaultPortKind,
  deploymentFormSchema,
  MEM_UNLIMITED_GB,
  normalizeContainerPath,
} from "@/lib/deployment-form"

describe("deployment volume policy", () => {
  it("accepts a server mount with purge-on-redeploy", () => {
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
          placement: "SERVER",
          purge_on_redeploy: true,
        },
      ],
    })
    expect(parsed.success).toBe(true)
  })

  it("rejects an unknown placement string", () => {
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
          placement: "CLUSTER",
          purge_on_redeploy: false,
        },
      ],
    })
    expect(parsed.success).toBe(false)
  })

  it.each(["/data:ro", "/data/../host", "/data/./models", "/data\u0000models"])(
    "rejects unsafe Docker destination %j",
    (containerPath) => {
      const parsed = deploymentFormSchema.safeParse({
        name: "",
        ports: [],
        env: [],
        mem_limit_gb: MEM_UNLIMITED_GB,
        volumes: [
          {
            volume_name: "models",
            container_path: containerPath,
            read_only: false,
            placement: "SERVER",
            purge_on_redeploy: false,
          },
        ],
      })
      expect(parsed.success).toBe(false)
    },
  )

  it("rejects duplicate normalized container destinations", () => {
    const parsed = deploymentFormSchema.safeParse({
      name: "",
      ports: [],
      env: [],
      mem_limit_gb: MEM_UNLIMITED_GB,
      volumes: [
        {
          volume_name: "models",
          container_path: "/data/models/",
          read_only: false,
          placement: "SERVER",
          purge_on_redeploy: false,
        },
        {
          volume_name: "cache",
          container_path: " /data//models ",
          read_only: false,
          placement: "SLOT",
          purge_on_redeploy: false,
        },
      ],
    })

    expect(parsed.success).toBe(false)
    if (!parsed.success) {
      expect(parsed.error.issues).toContainEqual(
        expect.objectContaining({ path: ["volumes", 1, "container_path"] }),
      )
    }
    expect(normalizeContainerPath(" /data//models/ ")).toBe("/data/models")
  })

  it("recognizes ComfyUI's exposed 8188 as an HTTP app", () => {
    expect(defaultPortKind({ container_port: 8188, protocol: "tcp" }, true)).toBe("HTTP")
    expect(defaultPortKind({ container_port: 8188, protocol: "tcp" }, false)).toBe("TCP")
  })
})
