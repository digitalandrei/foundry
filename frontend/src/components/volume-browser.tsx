import { useCallback, useEffect, useMemo, useState } from "react"
import { FilesIcon, SaveIcon } from "lucide-react"
import { toast } from "sonner"

import {
  type FileDrag,
  VolumeFilePane,
  type VolumePaneModel,
} from "@/components/volume-file-pane"
import { useConfirm } from "@/components/confirm-context"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { useVolumeFiles } from "@/hooks/use-volume-files"
import type { ServerSummary, ServerVolume } from "@/lib/types"
import {
  agentSupportsVolumeFiles,
  joinVolumePath,
  parentVolumePath,
  type VolumeFileEntry,
} from "@/lib/volume-files"
import { cn } from "@/lib/utils"

type Side = "left" | "right"
type NamingAction = { side: Side; kind: "folder" | "rename"; value: string }
type Editor = {
  side: Side
  volumeId: string
  path: string
  name: string
  content: string
  saving: boolean
}

function pane(volumeId = ""): VolumePaneModel {
  return { volumeId, path: "", entries: [], selectedPath: null, loading: false }
}

/** Dual-pane placement storage browser. The two panes share one authorized
 * reverse-WS session, so cross-volume copy/move stays on the GPU server. */
export function VolumeBrowser({
  server,
  volumes,
  deploymentId,
}: {
  server: ServerSummary
  volumes: ServerVolume[]
  deploymentId?: string
}) {
  const supported = agentSupportsVolumeFiles(server.agent_version)
  const files = useVolumeFiles(server.id, supported && volumes.length > 0, deploymentId)
  const connected = files.status === "open"
  const [left, setLeft] = useState<VolumePaneModel>(() => pane(volumes[0]?.id))
  const [right, setRight] = useState<VolumePaneModel>(() => pane(volumes[1]?.id ?? volumes[0]?.id))
  const [naming, setNaming] = useState<NamingAction | null>(null)
  const [editor, setEditor] = useState<Editor | null>(null)
  const confirm = useConfirm()

  useEffect(() => {
    const ids = new Set(volumes.map((volume) => volume.id))
    // The accessible-root list is external query state. If a root was
    // deleted while this browser is open, move that pane to a live root.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setLeft((current) => (ids.has(current.volumeId) ? current : pane(volumes[0]?.id)))
    setRight((current) =>
      ids.has(current.volumeId) ? current : pane(volumes[1]?.id ?? volumes[0]?.id),
    )
  }, [volumes])

  const updatePane = useCallback((side: Side, update: (current: VolumePaneModel) => VolumePaneModel) => {
    if (side === "left") setLeft(update)
    else setRight(update)
  }, [])

  const refresh = useCallback(
    async (side: Side) => {
      const current = side === "left" ? left : right
      if (!connected || !current.volumeId) return
      updatePane(side, (value) => ({ ...value, loading: true }))
      try {
        const entries = await files.list(current.volumeId, current.path)
        updatePane(side, (value) => ({
          ...value,
          entries,
          selectedPath: entries.some((entry) => entry.path === value.selectedPath)
            ? value.selectedPath
            : null,
          loading: false,
        }))
      } catch (error) {
        updatePane(side, (value) => ({ ...value, loading: false }))
        toast.error(error instanceof Error ? error.message : "Could not read directory")
      }
    },
    [connected, files, left, right, updatePane],
  )

  useEffect(() => {
    if (!connected) return
    // Directory contents come from the agent-backed external session.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    void refresh("left")
    void refresh("right")
    // Refresh is intentionally keyed by connection/root navigation below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [connected, left.volumeId, left.path, right.volumeId, right.path])

  const refreshBoth = useCallback(async () => {
    await Promise.all([refresh("left"), refresh("right")])
  }, [refresh])

  const model = (side: Side) => (side === "left" ? left : right)
  const other = (side: Side) => (side === "left" ? right : left)
  const selected = (side: Side): VolumeFileEntry | undefined => {
    const current = model(side)
    return current.entries.find((entry) => entry.path === current.selectedPath)
  }

  const run = async (operation: () => Promise<void>, success: string) => {
    try {
      await operation()
      toast.success(success)
      await refreshBoth()
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "File operation failed")
    }
  }

  const openEntry = async (side: Side, entry: VolumeFileEntry) => {
    if (entry.kind === "directory") {
      updatePane(side, (current) => ({
        ...current,
        path: entry.path,
        selectedPath: null,
      }))
    } else if (entry.kind === "file") {
      await openEditor(side, entry)
    } else {
      toast.error("Symlinks are shown but cannot be followed.")
    }
  }

  const openEditor = async (side: Side, entry = selected(side)) => {
    if (!entry || entry.kind !== "file") return
    const current = model(side)
    try {
      const content = await files.readText(current.volumeId, entry.path)
      setEditor({
        side,
        volumeId: current.volumeId,
        path: entry.path,
        name: entry.name,
        content,
        saving: false,
      })
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "This file cannot be edited")
    }
  }

  const copyOrMove = async (side: Side, kind: "copy" | "move") => {
    const entry = selected(side)
    if (!entry) return
    const source = model(side)
    const destination = other(side)
    const to = joinVolumePath(destination.path, entry.name)
    await run(
      () =>
        kind === "copy"
          ? files.copy(source.volumeId, entry.path, destination.volumeId, to)
          : files.move(source.volumeId, entry.path, destination.volumeId, to),
      `${entry.name} ${kind === "copy" ? "copied" : "moved"}`,
    )
  }

  const dropEntry = async (side: Side, drag: FileDrag, directory: string) => {
    const destination = model(side)
    await run(
      () =>
        files.copy(
          drag.volumeId,
          drag.path,
          destination.volumeId,
          joinVolumePath(directory, drag.name),
        ),
      `${drag.name} copied`,
    )
  }

  const upload = async (side: Side, list: FileList) => {
    const current = model(side)
    try {
      for (const file of Array.from(list)) {
        await files.upload(
          current.volumeId,
          joinVolumePath(current.path, file.name),
          file,
        )
      }
      toast.success(`${list.length} file${list.length === 1 ? "" : "s"} uploaded`)
      await refreshBoth()
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Upload failed")
    }
  }

  const remove = async (side: Side) => {
    const entry = selected(side)
    if (!entry) return
    const accepted = await confirm({
      title: `Delete ${entry.name}?`,
      description:
        entry.kind === "directory"
          ? "This recursively deletes the directory and every file below it."
          : "This permanently deletes the selected file.",
      confirmLabel: "DELETE",
      destructive: true,
      requireConfirmText: entry.name,
    })
    if (!accepted) return
    const current = model(side)
    await run(() => files.remove(current.volumeId, entry.path), `${entry.name} deleted`)
  }

  const download = async (side: Side) => {
    const entry = selected(side)
    if (!entry || entry.kind !== "file") return
    const current = model(side)
    try {
      await files.download(current.volumeId, entry.path, entry.name)
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Download failed")
    }
  }

  const paneProps = (side: Side) => {
    const current = model(side)
    return {
      side,
      volumes,
      model: current,
      connected,
      onVolume: (volumeId: string) =>
        updatePane(side, () => pane(volumeId)),
      onPath: (path: string) =>
        updatePane(side, (value) => ({ ...value, path, selectedPath: null })),
      onSelect: (selectedPath: string | null) =>
        updatePane(side, (value) => ({ ...value, selectedPath })),
      onRefresh: () => void refresh(side),
      onOpen: (entry: VolumeFileEntry) => void openEntry(side, entry),
      onNewFolder: () => setNaming({ side, kind: "folder", value: "" }),
      onRename: () => {
        const entry = selected(side)
        if (entry) setNaming({ side, kind: "rename", value: entry.name })
      },
      onEdit: () => void openEditor(side),
      onCopy: () => void copyOrMove(side, "copy"),
      onMove: () => void copyOrMove(side, "move"),
      onDelete: () => void remove(side),
      onDownload: () => void download(side),
      onUpload: (list: FileList) => void upload(side, list),
      onDropEntry: (drag: FileDrag, directory: string) =>
        void dropEntry(side, drag, directory),
    }
  }

  const statusText = useMemo(() => {
    if (!supported) {
      return `Upgrade ${server.name} from agent ${server.agent_version ?? "unknown"} to 0.63.0`
    }
    if (files.status === "connecting") return "Connecting to server agent…"
    if (files.status === "open") return "Connected · changes are live"
    if (files.status === "error" || files.status === "closed") {
      return files.note ?? "File session unavailable"
    }
    return "Not connected"
  }, [files.note, files.status, server.agent_version, server.name, supported])

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <FilesIcon className="size-4" aria-hidden />
        <h2 className="text-sm font-medium">Volume Files</h2>
        <span
          className={cn(
            "size-2 rounded-full",
            connected ? "bg-slot-free" : files.status === "connecting" ? "bg-slot-reserved" : "bg-slot-offline",
          )}
          aria-hidden
        />
        <span className="text-xs text-muted-foreground">{statusText}</span>
        <span className="ml-auto text-xs text-muted-foreground">
          Drag between panes to copy · drop desktop files to upload
        </span>
      </div>
      <div className="flex flex-col gap-3 xl:flex-row">
        <VolumeFilePane {...paneProps("left")} />
        <VolumeFilePane {...paneProps("right")} />
      </div>

      <Dialog open={naming !== null} onOpenChange={(open) => !open && setNaming(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{naming?.kind === "folder" ? "New folder" : "Rename entry"}</DialogTitle>
            <DialogDescription>
              Names stay inside the selected persistent volume directory.
            </DialogDescription>
          </DialogHeader>
          <Input
            autoFocus
            value={naming?.value ?? ""}
            onChange={(event) =>
              setNaming((current) => (current ? { ...current, value: event.target.value } : null))
            }
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault()
                document.getElementById("volume-name-submit")?.click()
              }
            }}
          />
          <DialogFooter>
            <Button variant="outline" onClick={() => setNaming(null)}>
              Cancel
            </Button>
            <Button
              id="volume-name-submit"
              disabled={!naming?.value.trim() || naming.value.includes("/")}
              onClick={() => {
                if (!naming) return
                const current = model(naming.side)
                const value = naming.value.trim()
                const entry = selected(naming.side)
                setNaming(null)
                if (naming.kind === "folder") {
                  void run(
                    () => files.mkdir(current.volumeId, joinVolumePath(current.path, value)),
                    `${value} created`,
                  )
                } else if (entry) {
                  void run(
                    () =>
                      files.rename(
                        current.volumeId,
                        entry.path,
                        joinVolumePath(parentVolumePath(entry.path), value),
                      ),
                    `${entry.name} renamed`,
                  )
                }
              }}
            >
              Save
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={editor !== null} onOpenChange={(open) => !open && setEditor(null)}>
        <DialogContent className="flex h-[85vh] max-w-[min(96vw,80rem)] flex-col">
          <DialogHeader>
            <DialogTitle className="font-mono">{editor?.name}</DialogTitle>
            <DialogDescription className="font-mono">
              /{editor?.path} · UTF-8 text · maximum 2 MiB
            </DialogDescription>
          </DialogHeader>
          <textarea
            aria-label="Text file content"
            spellCheck={false}
            className="min-h-0 flex-1 resize-none rounded-md border bg-background p-3 font-mono text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
            value={editor?.content ?? ""}
            onChange={(event) =>
              setEditor((current) =>
                current ? { ...current, content: event.target.value } : null,
              )
            }
          />
          <DialogFooter>
            <Button variant="outline" onClick={() => setEditor(null)}>
              Close
            </Button>
            <Button
              disabled={!editor || editor.saving}
              onClick={async () => {
                if (!editor) return
                setEditor({ ...editor, saving: true })
                try {
                  await files.writeText(editor.volumeId, editor.path, editor.content)
                  toast.success(`${editor.name} saved`)
                  setEditor(null)
                  await refreshBoth()
                } catch (error) {
                  setEditor((current) => (current ? { ...current, saving: false } : null))
                  toast.error(error instanceof Error ? error.message : "Save failed")
                }
              }}
            >
              <SaveIcon className="size-3.5" />
              {editor?.saving ? "Saving…" : "Save"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
