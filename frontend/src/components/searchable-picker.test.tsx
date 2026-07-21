import "@testing-library/jest-dom/vitest"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { SearchablePicker } from "@/components/searchable-picker"
import { searchablePickerMatches } from "@/lib/searchable-picker"
import { ServerPicker } from "@/components/server-picker"
import { VolumeLocationPicker } from "@/components/volume-location-picker"
import { volumeLocation } from "@/lib/volume-locations"
import type { ServerSummary, ServerVolume } from "@/lib/types"

afterEach(cleanup)

const server = { name: "Atlas", hostname: "atlas-gpu.internal" }

const volumes: ServerVolume[] = [
  {
    id: "shared-models",
    name: "models",
    path: "/storage/containers/volumes/shared-models",
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
    attachments: [
      {
        deployment_id: "deployment-inference",
        deployment_name: "inference-api-7f",
        state: "RUNNING",
        container_path: "/data/models",
        read_only: true,
        purge_on_redeploy: true,
      },
    ],
    created_at: "2026-07-21T00:00:00Z",
  },
  {
    id: "slot-cache",
    name: "cache",
    path: "/storage/containers/volumes/slot-cache",
    used_bytes: 0,
    quota_bytes: null,
    usage_measured_at: null,
    project_name: "worker",
    placement: "SLOT",
    slot_id: "slot-1",
    slot_name: "GPU 1",
    gpu_group_id: null,
    group_name: null,
    created_by_name: "Andrei",
    can_manage: true,
    attached_to: [],
    created_at: "2026-07-21T00:00:00Z",
  },
]

describe("searchable picker", () => {
  it("searches nodes by hostname as well as display name", () => {
    const nodes = [
      {
        id: "node-atlas",
        name: "Atlas",
        hostname: "atlas-gpu.internal",
        status: "ONLINE",
      },
      {
        id: "node-orion",
        name: "Orion",
        hostname: "orion-gpu.internal",
        status: "ONLINE",
      },
    ] as ServerSummary[]

    render(<ServerPicker value="node-atlas" servers={nodes} onValueChange={vi.fn()} />)
    fireEvent.click(screen.getByRole("combobox", { name: "Storage node" }))
    fireEvent.change(
      screen.getByRole("searchbox", { name: "Search nodes by name or hostname…" }),
      { target: { value: "orion-gpu" } },
    )

    expect(screen.getByRole("option", { name: "Orion" })).toBeVisible()
    expect(screen.queryByRole("option", { name: "Atlas" })).not.toBeInTheDocument()
  })

  it("matches all hierarchy terms, including server hostname", () => {
    const location = volumeLocation(volumes[0], server)
    const option = {
      value: volumes[0].id,
      label: location.breadcrumb,
      searchText: location.searchText,
      group: `${server.name} / ${location.placement}`,
      subgroup: location.project,
    }

    expect(searchablePickerMatches(option, "atlas-gpu shared inference models")).toBe(true)
    expect(searchablePickerMatches(option, "inference-api-7f")).toBe(true)
    expect(searchablePickerMatches(option, "data models running read only purge")).toBe(true)
    expect(searchablePickerMatches(option, "owner andrei")).toBe(true)
    expect(searchablePickerMatches(option, "atlas-gpu cache")).toBe(false)
  })

  describe.each(["light", "dark"])("%s theme", (theme) => {
    it("renders the server, placement, project, and mount hierarchy", () => {
      render(
        <div className={theme}>
          <VolumeLocationPicker
            value={volumes[0].id}
            volumes={volumes}
            server={server}
            onValueChange={vi.fn()}
            ariaLabel="Persistent root"
          />
        </div>,
      )

      fireEvent.click(screen.getByRole("combobox", { name: "Persistent root" }))

      expect(screen.getByText("Atlas / Shared")).toBeInTheDocument()
      expect(screen.getByText("Project inference-api")).toBeInTheDocument()
      expect(screen.getByText("Mount models")).toBeInTheDocument()
      expect(screen.getByRole("combobox", { name: "Persistent root" })).toHaveTextContent(
        "Atlas / Shared / Project inference-api / Mount models",
      )
    })

    it("announces detailed source safety context without making the trigger noisy", () => {
      render(
        <div className={theme}>
          <VolumeLocationPicker
            value={volumes[0].id}
            volumes={volumes}
            server={server}
            onValueChange={vi.fn()}
            ariaLabel="Persistent root"
            showDetails
          />
        </div>,
      )

      fireEvent.click(screen.getByRole("combobox", { name: "Persistent root" }))

      expect(
        screen.getByRole("option", { name: /active or retained binding.*owner andrei/i }),
      ).toBeVisible()
      expect(screen.getByRole("combobox", { name: "Persistent root" })).toHaveTextContent(
        "Atlas / Shared / Project inference-api / Mount models",
      )
    })

    it("selects the active result with the keyboard", () => {
      const change = vi.fn()
      render(
        <div className={theme}>
          <SearchablePicker
            value="first"
            options={[
              { value: "first", label: "First root" },
              { value: "second", label: "Second root" },
            ]}
            onValueChange={change}
            ariaLabel="Test picker"
            placeholder="Choose a root"
            searchPlaceholder="Search roots"
          />
        </div>,
      )

      fireEvent.click(screen.getByRole("combobox", { name: "Test picker" }))
      const search = screen.getByRole("searchbox", { name: "Search roots" })
      fireEvent.keyDown(search, { key: "ArrowDown" })
      fireEvent.keyDown(search, { key: "Enter" })

      expect(change).toHaveBeenCalledWith("second")
    })
  })
})
