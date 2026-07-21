import "@testing-library/jest-dom/vitest"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { useFieldArray, useForm, useWatch } from "react-hook-form"
import { afterEach, describe, expect, it } from "vitest"

import { PersistentMountRow } from "@/components/persistent-mount-row"
import {
  MEM_UNLIMITED_GB,
  type DeploymentFormValues,
} from "@/lib/deployment-form"
import type { ServerVolume } from "@/lib/types"

afterEach(cleanup)

const server = { name: "Atlas", hostname: "atlas-gpu.internal" }
const crossProjectVolume: ServerVolume = {
  id: "shared-models",
  name: "models",
  path: "/storage/containers/.foundry/shared/comfy-library/models/volume",
  used_bytes: 4 * 1024 ** 3,
  quota_bytes: 8 * 1024 ** 3,
  usage_measured_at: "2026-07-21T00:00:00Z",
  project_name: "comfy-library",
  placement: "SERVER",
  slot_id: null,
  slot_name: null,
  gpu_group_id: null,
  group_name: null,
  created_by_name: "Andrei",
  can_manage: true,
  attached_to: ["comfy-production"],
  attachments: [
    {
      deployment_id: "deployment-1",
      deployment_name: "comfy-production",
      state: "RUNNING",
      container_path: "/data/models",
      read_only: false,
      purge_on_redeploy: true,
    },
    {
      deployment_id: "deployment-2",
      deployment_name: "comfy-old",
      state: "STOPPED",
      container_path: "/models",
      read_only: true,
      purge_on_redeploy: false,
    },
  ],
  created_at: "2026-07-21T00:00:00Z",
}

function Harness({
  defaultVolume = {
    volume_id: null,
    volume_name: "settings",
    container_path: "/data/settings",
    read_only: false,
    placement: "SLOT" as const,
    purge_on_redeploy: false,
  },
}: {
  defaultVolume?: DeploymentFormValues["volumes"][number]
}) {
  const form = useForm<DeploymentFormValues>({
    defaultValues: {
      name: "new-comfy",
      ports: [],
      env: [],
      volumes: [defaultVolume],
      mem_limit_gb: MEM_UNLIMITED_GB,
    },
  })
  const mounts = useFieldArray({ control: form.control, name: "volumes" })
  const values = useWatch({ control: form.control })
  const mount = values.volumes?.[0] ?? defaultVolume
  const selectedId = mount.volume_id ?? null
  const selectedVolume = selectedId === crossProjectVolume.id ? crossProjectVolume : null
  return (
    <div>
      <PersistentMountRow
        index={0}
        fieldId={mounts.fields[0].id}
        form={form}
        mounts={mounts}
        mount={mount}
        selectedId={selectedId}
        selectedVolume={selectedVolume}
        availableVolumes={[crossProjectVolume]}
        storageServer={server}
        projectName="new-comfy"
        loading={false}
        error={null}
        replacementDeploymentId={null}
      />
      <output data-testid="mount-values">{JSON.stringify(values.volumes)}</output>
    </div>
  )
}

describe.each(["light", "dark"])("persistent mount mapping (%s theme)", (theme) => {
  it("reuses a cross-project source while keeping its destination independent", () => {
    render(<div className={theme}><Harness /></div>)

    fireEvent.click(screen.getByRole("combobox", { name: "Storage source for mount 1" }))
    fireEvent.click(screen.getByRole("option", { name: /project comfy-library.*mount models/i }))

    expect(screen.getByText("Existing source stays authoritative")).toBeVisible()
    expect(
      screen.getByRole("combobox", { name: "Storage source for mount 1" }),
    ).toHaveTextContent("Atlas / Shared / Project comfy-library / Mount models")
    expect(screen.getByText("Known mappings")).toBeVisible()
    const mappings = screen.getByRole("list", { name: "Known volume mappings" })
    expect(mappings).toHaveTextContent("comfy-production")
    expect(mappings).toHaveTextContent("/data/models")
    expect(mappings).toHaveTextContent("read-write")
    expect(mappings).toHaveTextContent("running")
    expect(mappings).toHaveTextContent("stopped")
    expect(mappings).toHaveTextContent("purges on redeploy")
    expect(screen.getByDisplayValue("/data/settings")).toBeVisible()

    fireEvent.change(screen.getByLabelText("Container destination"), {
      target: { value: "/workspace/models" },
    })
    expect(screen.getByTestId("mount-values")).toHaveTextContent('"volume_id":"shared-models"')
    expect(screen.getByTestId("mount-values")).toHaveTextContent('"volume_name":"models"')
    expect(screen.getByTestId("mount-values")).toHaveTextContent('"container_path":"/workspace/models"')
  })

  it("switches an existing root back to editable automatic storage", () => {
    render(
      <div className={theme}>
        <Harness
          defaultVolume={{
            volume_id: "shared-models",
            volume_name: "models",
            container_path: "/data/models",
            read_only: false,
            placement: "SERVER",
            purge_on_redeploy: false,
          }}
        />
      </div>,
    )

    fireEvent.click(screen.getByRole("combobox", { name: "Storage source for mount 1" }))
    fireEvent.click(screen.getByRole("option", { name: /create.*reuse automatically/i }))

    expect(screen.getByLabelText("New mount name")).toHaveValue("models")
    expect(screen.getAllByText(/Project new-comfy \/ Mount models/)).not.toHaveLength(0)
    expect(screen.getByTestId("mount-values")).toHaveTextContent('"volume_id":null')
  })

  it("ranks likely compatible roots without selecting one", () => {
    render(
      <div className={theme}>
        <Harness
          defaultVolume={{
            volume_id: null,
            volume_name: "models",
            container_path: "/data/models",
            read_only: false,
            placement: "SLOT",
            purge_on_redeploy: false,
          }}
        />
      </div>,
    )

    expect(screen.getByText(/Suggested:/)).toHaveTextContent(
      "comfy-library/models (same mount name, previously mapped to this container path)",
    )
    expect(screen.getByTestId("mount-values")).toHaveTextContent('"volume_id":null')
    fireEvent.click(screen.getByRole("combobox", { name: "Storage source for mount 1" }))
    expect(screen.getByText("Suggested for this mapping")).toBeVisible()
  })

  it("blocks purge while a shared source has active or retained references", () => {
    render(
      <div className={theme}>
        <Harness
          defaultVolume={{
            volume_id: "shared-models",
            volume_name: "models",
            container_path: "/data/models",
            read_only: false,
            placement: "SERVER",
            purge_on_redeploy: true,
          }}
        />
      </div>,
    )

    expect(screen.getByRole("note")).toHaveTextContent("Referenced by comfy-production")
    expect(screen.getByRole("note")).toHaveTextContent(
      "Purge is disabled while another active or retained deployment references this root",
    )
    expect(screen.getByRole("checkbox", { name: /Purge unavailable/ })).toBeDisabled()
    expect(screen.getByRole("checkbox", { name: /Purge unavailable/ })).not.toBeChecked()
  })

  it("keeps an unavailable replacement source explicit until remapped", () => {
    render(
      <div className={theme}>
        <Harness
          defaultVolume={{
            volume_id: "legacy-source",
            volume_name: "models",
            container_path: "/data/models",
            read_only: false,
            placement: "SERVER",
            purge_on_redeploy: false,
          }}
        />
      </div>,
    )

    const legacyStatus = screen
      .getAllByRole("status")
      .find((status) => status.textContent?.includes("Current source unavailable / legacy"))
    expect(legacyStatus).toHaveTextContent("Current source unavailable / legacy")
    expect(legacyStatus).toHaveTextContent("will not be recreated")
    expect(screen.queryByLabelText("New mount name")).not.toBeInTheDocument()
    expect(screen.getByTestId("mount-values")).toHaveTextContent('"volume_id":"legacy-source"')
  })
})
