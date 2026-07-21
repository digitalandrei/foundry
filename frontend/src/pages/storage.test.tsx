import "@testing-library/jest-dom/vitest"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import type { ServerSummary, ServerVolume } from "@/lib/types"

const mocks = vi.hoisted(() => ({
  useServers: vi.fn(),
  useServerVolumes: vi.fn(),
}))

vi.mock("@/components/confirm-context", () => ({ useConfirm: () => vi.fn() }))
vi.mock("@/components/volume-browser", () => ({ VolumeBrowser: () => null }))
vi.mock("@/hooks/use-servers", () => ({ useServers: mocks.useServers }))
vi.mock("@/hooks/use-deployments", () => ({
  useCleanVolume: () => ({ isPending: false, mutate: vi.fn() }),
  useDeleteVolume: () => ({ isPending: false, mutate: vi.fn() }),
  useSetVolumeQuota: () => ({ isPending: false, mutate: vi.fn() }),
  useServerVolumes: mocks.useServerVolumes,
}))

import { StoragePage } from "@/pages/storage"

afterEach(cleanup)

const server = {
  id: "server-1",
  name: "Atlas",
  hostname: "atlas-gpu.internal",
  status: "ONLINE",
} as ServerSummary

const volumes: ServerVolume[] = [
  {
    id: "volume-attached",
    name: "models",
    path: "/storage/containers/volumes/models",
    used_bytes: 0,
    quota_bytes: null,
    usage_measured_at: null,
    project_name: "inference-api",
    placement: "SERVER",
    slot_id: null,
    slot_name: null,
    gpu_group_id: null,
    group_name: null,
    created_by_name: "Andrei",
    can_manage: true,
    attached_to: ["inference-api-7f"],
    created_at: "2026-07-21T00:00:00Z",
  },
  {
    id: "volume-private",
    name: "private-data",
    path: "/storage/containers/volumes/private-data",
    used_bytes: 0,
    quota_bytes: null,
    usage_measured_at: null,
    project_name: "private-worker",
    placement: "SLOT",
    slot_id: "slot-1",
    slot_name: "GPU 1",
    gpu_group_id: null,
    group_name: null,
    created_by_name: "Another operator",
    can_manage: false,
    attached_to: [],
    created_at: "2026-07-21T00:00:00Z",
  },
]

beforeEach(() => {
  mocks.useServers.mockReturnValue({ data: [server], isPending: false, isError: false })
  mocks.useServerVolumes.mockReturnValue({ data: volumes, isPending: false, isError: false })
})

describe("StoragePage", () => {
  it("filters policies by attachment and explains disabled actions", () => {
    render(<StoragePage />)

    const search = screen.getByRole("searchbox", { name: "Search volume policies" })
    fireEvent.change(search, { target: { value: "inference-api-7f" } })
    expect(screen.getByText("Atlas / Shared / Project inference-api / Mount models")).toBeVisible()
    expect(screen.queryByText("Mount private-data")).not.toBeInTheDocument()

    fireEvent.change(search, { target: { value: "does-not-exist" } })
    expect(screen.getByText("No matching volume policies")).toBeVisible()

    fireEvent.change(search, { target: { value: "" } })
    const attachedReason = screen.getByText(
      "This volume is attached to a deployment. Detach it before you can clean it.",
    )
    const clean = screen.getAllByRole("button", { name: "Clean" })[0]
    expect(clean).toBeDisabled()
    expect(clean).toHaveAttribute("aria-describedby", attachedReason.id)

    const privateDelete = screen.getByRole("button", { name: "Delete volume private-data" })
    const ownershipReason = screen.getByText(
      "Only the volume creator or an administrator can delete this volume.",
    )
    expect(privateDelete).toBeDisabled()
    expect(privateDelete).toHaveAttribute("aria-describedby", ownershipReason.id)
  })

  it("shows the no-server state before the disabled volume query skeleton", () => {
    mocks.useServers.mockReturnValue({ data: [], isPending: false, isError: false })
    mocks.useServerVolumes.mockReturnValue({ data: undefined, isPending: true, isError: false })

    render(<StoragePage />)

    expect(screen.getByText("Choose a server")).toBeVisible()
    expect(screen.queryByText("No volumes in this scope")).not.toBeInTheDocument()
  })
})
