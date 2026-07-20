import "@testing-library/jest-dom/vitest"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { VolumeFilePane, type VolumePaneModel } from "@/components/volume-file-pane"
import type { ServerVolume } from "@/lib/types"

afterEach(cleanup)

const volume: ServerVolume = {
  id: "volume-1",
  name: "models",
  path: "/storage/containers/volumes/volume-1",
  project_id: "project-1",
  project_name: "templates/comfyui",
  visibility: "PROJECT",
  placement: "SERVER",
  slot_id: null,
  slot_name: null,
  created_by_name: "Andrei",
  can_manage: true,
  attached_to: [],
  created_at: "2026-07-20T00:00:00Z",
}

function model(): VolumePaneModel {
  return {
    volumeId: volume.id,
    path: "",
    selectedPath: null,
    loading: false,
    entries: [
      {
        name: "checkpoints",
        path: "checkpoints",
        kind: "directory",
        size: 0,
        modified_at: null,
      },
      {
        name: "settings.json",
        path: "settings.json",
        kind: "file",
        size: 42,
        modified_at: null,
      },
    ],
  }
}

describe.each(["light", "dark"])("volume file pane (%s theme)", (theme) => {
  it("names its file list and opens an entry with the keyboard", () => {
    const open = vi.fn()
    render(
      <div className={theme}>
        <VolumeFilePane
          side="left"
          volumes={[volume]}
          model={model()}
          connected
          onVolume={vi.fn()}
          onPath={vi.fn()}
          onSelect={vi.fn()}
          onRefresh={vi.fn()}
          onOpen={open}
          onNewFolder={vi.fn()}
          onRename={vi.fn()}
          onEdit={vi.fn()}
          onCopy={vi.fn()}
          onMove={vi.fn()}
          onDelete={vi.fn()}
          onDownload={vi.fn()}
          onUpload={vi.fn()}
          onDropEntry={vi.fn()}
        />
      </div>,
    )
    expect(screen.getByRole("listbox", { name: "Files in /" })).toBeInTheDocument()
    const directory = screen.getByRole("option", { name: /checkpoints/i })
    directory.focus()
    fireEvent.keyDown(directory, { key: "Enter" })
    expect(open).toHaveBeenCalledWith(expect.objectContaining({ path: "checkpoints" }))
  })
})
