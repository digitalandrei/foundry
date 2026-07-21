import "@testing-library/jest-dom/vitest"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { DeployDialog, type DeployTarget } from "@/components/deploy-dialog"
import type { DeploymentSummary } from "@/lib/types"

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

beforeEach(() => {
  vi.stubGlobal("ResizeObserver", class {
    observe() {}
    unobserve() {}
    disconnect() {}
  })
})

const replacement: DeploymentSummary = {
  id: "deployment-old",
  name: "existing-comfy",
  image_ref: "registry.example/comfy:v1",
  image_digest: null,
  state: "RUNNING",
  status_detail: null,
  container_id: "container-old",
  error_message: null,
  health_status: null,
  health_detail: null,
  server_id: "server-atlas",
  server_name: "Atlas",
  slot_id: "slot-1",
  slot_name: "GPU 1",
  slot_ids: ["slot-1"],
  gpu_group_id: null,
  group_name: null,
  gpu_label: "GPU 0",
  created_by_name: "Andrei",
  ports: [],
  created_at: "2026-07-21T00:00:00Z",
  started_at: "2026-07-21T00:01:00Z",
  adopted: false,
}

const target: DeployTarget = {
  tag: {
    registryTagId: "registry-tag-1",
    imageName: "ComfyUI",
    tagName: "latest",
    sizeBytes: null,
  },
  slot: {
    kind: "slot",
    slotId: "slot-1",
    slotName: "GPU 1",
    slotState: "RUNNING",
    serverId: "server-atlas",
    serverName: "Atlas",
    replace: true,
  },
  group: null,
  replaces: replacement,
}

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  })
}

function imageMetadata() {
  return {
    project_id: null,
    ports: [],
    volumes: [{
      volume_id: null,
      volume_name: "image-default",
      container_path: "/image/default",
      read_only: false,
      placement: "SLOT",
      purge_on_redeploy: false,
    }],
    size_bytes: null,
    digest: null,
    apps: [],
  }
}

function renderDialog() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      <DeployDialog target={target} onClose={vi.fn()} />
    </QueryClientProvider>,
  )
}

function stableResponse(url: string) {
  if (url === "/api/me") {
    return json({
      id: "user-andrei",
      display_name: "Andrei",
      email: null,
      avatar_url: null,
      is_admin: true,
      accounts: [],
      apps_domain: null,
    })
  }
  if (url === "/api/servers") return json([{ id: "server-atlas", name: "Atlas", hostname: null }])
  if (url === "/api/registry/tags/registry-tag-1/metadata") return json(imageMetadata())
  if (url.startsWith("/api/servers/server-atlas/volumes")) return json([])
  throw new Error(`unexpected request ${url}`)
}

describe("replacement deploy dialog", () => {
  it("blocks the form until exact predecessor mounts are loaded, then pre-fills them", async () => {
    let resolveDetail!: (response: Response) => void
    const detailResponse = new Promise<Response>((resolve) => {
      resolveDetail = resolve
    })
    vi.stubGlobal("fetch", vi.fn((input: RequestInfo | URL) => {
      const url = String(input)
      return url === "/api/deployments/deployment-old"
        ? detailResponse
        : Promise.resolve(stableResponse(url))
    }))

    renderDialog()

    expect(
      await screen.findByText("Loading the deployment configuration to preserve its persistent mount mappings…"),
    ).toBeVisible()
    expect(screen.queryByLabelText("Name")).not.toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Replace" })).not.toBeInTheDocument()

    await act(async () => {
      resolveDetail(json({
        ...replacement,
        mounts: [{
          volume_id: "volume-models",
          volume_name: "models",
          host_path: "/storage/containers/.foundry/shared/existing-comfy/models/volume-models",
          container_path: "/data/models",
          read_only: true,
          placement: "SERVER",
          purge_on_redeploy: false,
        }],
        env: [],
      }))
    })

    await waitFor(() => {
      expect(screen.getByDisplayValue("existing-comfy")).toBeVisible()
    })
    expect(screen.getByDisplayValue("/data/models")).toBeVisible()
    expect(screen.getByRole("button", { name: "Replace" })).toBeEnabled()
  })

  it("keeps replacement submission unavailable after a detail error and retries explicitly", async () => {
    let detailAttempts = 0
    const fetchMock = vi.fn((input: RequestInfo | URL) => {
      const url = String(input)
      if (url === "/api/deployments/deployment-old") {
        detailAttempts += 1
        return Promise.resolve(
          detailAttempts === 1
            ? json({ error: { code: "unavailable", message: "detail unavailable" } }, 503)
            : json({ ...replacement, mounts: [], env: [] }),
        )
      }
      return Promise.resolve(stableResponse(url))
    })
    vi.stubGlobal("fetch", fetchMock)

    renderDialog()

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not load the deployment being replaced.",
    )
    expect(screen.queryByRole("button", { name: "Replace" })).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole("button", { name: "Retry loading configuration" }))

    await waitFor(() => {
      expect(screen.getByDisplayValue("existing-comfy")).toBeVisible()
    })
    expect(detailAttempts).toBe(2)
    expect(screen.queryByDisplayValue("/image/default")).not.toBeInTheDocument()
  })
})
