import { describe, it, expect } from "vitest"
import { slotDeployability, occupantsBySlot, gpuSlotPositions } from "@/lib/slots"
import type {
  DeploymentSummary,
  ExternalOccupant,
  GpuSummary,
  ServerSummary,
  SlotSummary,
} from "@/lib/types"

// Minimal fully-typed fixtures with sensible defaults; override per case.
function slot(over: Partial<SlotSummary> = {}): SlotSummary {
  return {
    id: "slot-1",
    name: "0",
    slot_type: "FULL_GPU",
    mig_uuid: null,
    mig_profile: null,
    capacity_mb: null,
    state: "FREE",
    max_occupants: 1,
    external: null,
    ...over,
  }
}

function server(over: Partial<ServerSummary> = {}): ServerSummary {
  return {
    id: "srv-1",
    name: "gpu-a",
    hostname: null,
    status: "ONLINE",
    last_heartbeat_at: null,
    agent_version: null,
    os_version: null,
    app_publishing_ready: null,
    nginx_status: null,
    docker_ok: true,
    setup_revision: null,
    required_setup_revision: 4,
    readiness: null,
    readiness_checked_at: null,
    storage_total_bytes: null,
    storage_available_bytes: null,
    enrolled: true,
    gpus: [],
    containers_running: 0,
    ...over,
  }
}

function deployment(over: Partial<DeploymentSummary> = {}): DeploymentSummary {
  return {
    id: "dep-1",
    name: "svc",
    image_ref: "reg/img:tag",
    image_digest: null,
    state: "RUNNING",
    status_detail: null,
    container_id: null,
    error_message: null,
    health_status: null,
    health_detail: null,
    server_id: "srv-1",
    server_name: "gpu-a",
    slot_id: "slot-1",
    slot_name: "0",
    slot_ids: ["slot-1"],
    gpu_group_id: null,
    group_name: null,
    gpu_label: "GPU 0",
    created_by_name: "op",
    ports: [],
    created_at: "2026-06-19T00:00:00Z",
    started_at: null,
    adopted: false,
    ...over,
  }
}

const external = (over: Partial<ExternalOccupant> = {}): ExternalOccupant => ({
  name: "rogue",
  image: "busybox",
  running: true,
  ...over,
})

describe("slotDeployability — single-use", () => {
  it("free slot on a healthy server is a fresh deploy", () => {
    expect(slotDeployability(slot({ state: "FREE" }), server(), [])).toEqual({
      deployable: true,
      replace: false,
      reason: null,
    })
  })

  it("running slot is deployable as a replace", () => {
    const d = slotDeployability(slot({ state: "RUNNING" }), server(), [deployment()])
    expect(d.deployable).toBe(true)
    expect(d.replace).toBe(true)
  })

  it("offline server blocks the deploy", () => {
    const d = slotDeployability(slot({ state: "FREE" }), server({ status: "OFFLINE" }), [])
    expect(d).toMatchObject({ deployable: false, reason: "server offline" })
  })

  it("docker down blocks the deploy", () => {
    const d = slotDeployability(slot({ state: "FREE" }), server({ docker_ok: false }), [])
    expect(d.deployable).toBe(false)
  })

  it("missing NVIDIA container support blocks the deploy", () => {
    const d = slotDeployability(
      slot({ state: "FREE" }),
      server({
        readiness: {
          setup_revision: 4,
          required_setup_revision: 4,
          checked_at: "2026-07-21T00:00:00Z",
          checks: [{ code: "docker_gpu", status: "FAILED", detail: "no NVIDIA CDI device" }],
        },
      }),
      [],
    )
    expect(d).toMatchObject({ deployable: false, reason: "no NVIDIA CDI device" })
  })

  it("an old agent report explains that NVIDIA support is unverified", () => {
    const d = slotDeployability(
      slot({ state: "FREE" }),
      server({
        readiness: {
          setup_revision: 4,
          required_setup_revision: 4,
          checked_at: "2026-07-21T00:00:00Z",
          checks: [{ code: "docker", status: "READY", detail: "Docker ready" }],
        },
      }),
      [],
    )
    expect(d).toMatchObject({
      deployable: false,
      reason: "NVIDIA container support has not been verified",
    })
  })

  it("a non-Foundry container holding the GPU blocks the deploy", () => {
    const d = slotDeployability(slot({ state: "FREE", external: external() }), server(), [])
    expect(d.deployable).toBe(false)
  })
})

describe("slotDeployability — multi-use", () => {
  it("takes another co-tenant under cap (never a replace)", () => {
    const d = slotDeployability(slot({ state: "RUNNING", max_occupants: 4 }), server(), [deployment()])
    expect(d).toEqual({ deployable: true, replace: false, reason: null })
  })

  it("is full at cap", () => {
    const occ = [deployment({ id: "a" }), deployment({ id: "b" })]
    const d = slotDeployability(slot({ state: "RUNNING", max_occupants: 2 }), server(), occ)
    expect(d.deployable).toBe(false)
    expect(d.reason).toMatch(/full/)
  })
})

function gpu(over: Partial<GpuSummary> = {}): GpuSummary {
  return {
    id: "gpu-3",
    gpu_uuid: "GPU-xxxx",
    index: 3,
    model: "RTX PRO 6000",
    memory_mb: 98304,
    mig_enabled: true,
    slots: [],
    groups: [],
    ...over,
  }
}

describe("gpuSlotPositions — labels", () => {
  it("labels each position with the slot's own name (full card = index, MIG = card.slice)", () => {
    const slots = [
      slot({ id: "s1", name: "3.1", slot_type: "MIG_SLOT" }),
      slot({ id: "s2", name: "3.2", slot_type: "MIG_SLOT" }),
      slot({ id: "s3", name: "3.3", slot_type: "MIG_SLOT" }),
      slot({ id: "s4", name: "3.4", slot_type: "MIG_SLOT" }),
    ]
    const labels = gpuSlotPositions(gpu({ slots }), new Map()).map((p) => p.label)
    expect(labels).toEqual(["3.1", "3.2", "3.3", "3.4"])
  })

  it("suffixes co-tenant positions on a multi-use slot to keep them distinct", () => {
    const slots = [slot({ id: "s1", name: "0", max_occupants: 3 })]
    const labels = gpuSlotPositions(gpu({ index: 0, mig_enabled: false, slots }), new Map()).map(
      (p) => p.label,
    )
    expect(labels).toEqual(["0 ·1", "0 ·2", "0 ·3"])
  })
})

describe("occupantsBySlot", () => {
  it("folds an occupant under every member slot, newest first", () => {
    const a = deployment({ id: "a", slot_ids: ["s1", "s2"], created_at: "2026-06-19T00:00:00Z" })
    const b = deployment({ id: "b", slot_ids: ["s1"], created_at: "2026-06-19T01:00:00Z" })
    const map = occupantsBySlot([a, b])
    expect(map.get("s1")?.map((d) => d.id)).toEqual(["b", "a"])
    expect(map.get("s2")?.map((d) => d.id)).toEqual(["a"])
  })

  it("ignores deployments that have left the host (REMOVED)", () => {
    const map = occupantsBySlot([deployment({ id: "x", state: "REMOVED" })])
    expect(map.size).toBe(0)
  })

  it("does not fold a group deploy onto its member slots (the group is independent)", () => {
    // A group deploy occupies the group, not the member GPUs' own slots —
    // member cards stay free for individual deploys.
    const g = deployment({ id: "g", gpu_group_id: "grp-1", slot_ids: ["s1", "s2"] })
    const map = occupantsBySlot([g])
    expect(map.size).toBe(0)
  })
})
