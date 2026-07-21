import { useRef } from "react"
import {
  ArrowDownToLineIcon,
  ArrowLeftRightIcon,
  ArrowUpIcon,
  CopyIcon,
  DownloadIcon,
  Edit3Icon,
  FileIcon,
  FolderIcon,
  FolderPlusIcon,
  RefreshCwIcon,
  Trash2Icon,
  UploadIcon,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import type { ServerVolume } from "@/lib/types"
import {
  formatFileSize,
  parentVolumePath,
  type VolumeFileEntry,
} from "@/lib/volume-files"
import { cn } from "@/lib/utils"

export const FOUNDRY_FILE_DRAG = "application/x-foundry-volume-file"

export type FileDrag = {
  volumeId: string
  path: string
  name: string
  kind: VolumeFileEntry["kind"]
}

export type VolumePaneModel = {
  volumeId: string
  path: string
  entries: VolumeFileEntry[]
  selectedPath: string | null
  loading: boolean
}

type Props = {
  side: "left" | "right"
  volumes: ServerVolume[]
  model: VolumePaneModel
  connected: boolean
  onVolume: (volumeId: string) => void
  onPath: (path: string) => void
  onSelect: (path: string | null) => void
  onRefresh: () => void
  onOpen: (entry: VolumeFileEntry) => void
  onNewFolder: () => void
  onRename: () => void
  onEdit: () => void
  onCopy: () => void
  onMove: () => void
  onDelete: () => void
  onDownload: () => void
  onUpload: (files: FileList) => void
  onDropEntry: (drag: FileDrag, directory: string) => void
}

/** One Midnight-Commander-style pane. Native file drag is used here so
 * desktop files can be dropped into the same surface as cross-pane items. */
export function VolumeFilePane({
  side,
  volumes,
  model,
  connected,
  onVolume,
  onPath,
  onSelect,
  onRefresh,
  onOpen,
  onNewFolder,
  onRename,
  onEdit,
  onCopy,
  onMove,
  onDelete,
  onDownload,
  onUpload,
  onDropEntry,
}: Props) {
  const inputRef = useRef<HTMLInputElement>(null)
  const selected = model.entries.find((entry) => entry.path === model.selectedPath)
  const isFile = selected?.kind === "file"
  const canChange = selected && selected.kind !== "symlink"

  const acceptDrop = (event: React.DragEvent, directory: string) => {
    event.preventDefault()
    if (event.dataTransfer.files.length > 0) {
      onUpload(event.dataTransfer.files)
      return
    }
    const raw = event.dataTransfer.getData(FOUNDRY_FILE_DRAG)
    if (!raw) return
    try {
      onDropEntry(JSON.parse(raw) as FileDrag, directory)
    } catch {
      // Ignore unrelated native drag payloads.
    }
  }

  return (
    <section
      className="min-w-0 flex-1 overflow-hidden rounded-md border bg-card"
      aria-label={`${side} volume pane`}
      onDragOver={(event) => event.preventDefault()}
      onDrop={(event) => acceptDrop(event, model.path)}
    >
      <div className="border-b bg-muted/35 p-2">
        <Select value={model.volumeId} onValueChange={onVolume}>
          <SelectTrigger className="h-8 font-mono text-xs" aria-label={`${side} volume`}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {volumes.map((volume) => (
              <SelectItem key={volume.id} value={volume.id}>
                {volume.name} ·{" "}
                {volume.placement === "SERVER"
                  ? "server shared"
                  : volume.gpu_group_id
                    ? `group ${volume.group_name ?? "?"}`
                    : `slot ${volume.slot_name ?? "?"}`}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <div className="mt-2 flex items-center gap-1">
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label="Parent directory"
            title="Parent directory"
            disabled={!connected || model.path === ""}
            onClick={() => onPath(parentVolumePath(model.path))}
          >
            <ArrowUpIcon className="size-3.5" />
          </Button>
          <span className="min-w-0 flex-1 truncate rounded-sm border bg-background px-2 py-1 font-mono text-xs">
            /{model.path}
          </span>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label="Refresh directory"
            title="Refresh"
            disabled={!connected || model.loading}
            onClick={onRefresh}
          >
            <RefreshCwIcon className={cn("size-3.5", model.loading && "animate-spin")} />
          </Button>
        </div>
      </div>

      <div className="h-80 overflow-auto font-mono text-xs sm:h-[26rem]">
        {model.loading ? (
          <p className="p-4 text-muted-foreground">Reading directory…</p>
        ) : model.entries.length === 0 ? (
          <div
            className="flex h-full items-center justify-center p-6 text-center text-muted-foreground"
            onDrop={(event) => acceptDrop(event, model.path)}
          >
            Empty directory — drop files here to upload
          </div>
        ) : (
          <div role="listbox" aria-label={`Files in /${model.path}`}>
            {model.entries.map((entry) => {
              const selectedRow = entry.path === model.selectedPath
              return (
                <div
                  key={entry.path}
                  role="option"
                  aria-selected={selectedRow}
                  tabIndex={0}
                  draggable={entry.kind !== "symlink"}
                  className={cn(
                    "grid cursor-default grid-cols-[minmax(0,1fr)_6rem] items-center gap-2 border-b px-2 py-1.5 outline-none hover:bg-accent/60 focus:bg-accent/60",
                    selectedRow && "bg-primary/15 text-foreground",
                    entry.kind === "symlink" && "text-muted-foreground",
                  )}
                  onClick={() => onSelect(entry.path)}
                  onDoubleClick={() => onOpen(entry)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") onOpen(entry)
                  }}
                  onDragStart={(event) => {
                    event.dataTransfer.effectAllowed = "copyMove"
                    event.dataTransfer.setData(
                      FOUNDRY_FILE_DRAG,
                      JSON.stringify({
                        volumeId: model.volumeId,
                        path: entry.path,
                        name: entry.name,
                        kind: entry.kind,
                      } satisfies FileDrag),
                    )
                  }}
                  onDragOver={
                    entry.kind === "directory"
                      ? (event) => {
                          event.preventDefault()
                          event.stopPropagation()
                        }
                      : undefined
                  }
                  onDrop={
                    entry.kind === "directory"
                      ? (event) => {
                          event.stopPropagation()
                          acceptDrop(event, entry.path)
                        }
                      : undefined
                  }
                >
                  <span className="flex min-w-0 items-center gap-1.5">
                    {entry.kind === "directory" ? (
                      <FolderIcon className="size-3.5 shrink-0 text-primary" aria-hidden />
                    ) : (
                      <FileIcon className="size-3.5 shrink-0" aria-hidden />
                    )}
                    <span className="truncate">{entry.name}</span>
                    {entry.kind === "symlink" ? (
                      <span className="text-[10px]">symlink</span>
                    ) : null}
                  </span>
                  <span className="text-right text-muted-foreground tabular-nums">
                    {entry.kind === "file" ? formatFileSize(entry.size) : "<DIR>"}
                  </span>
                </div>
              )
            })}
          </div>
        )}
      </div>

      <div className="flex flex-wrap gap-1 border-t bg-muted/35 p-2">
        <Button variant="outline" size="sm" disabled={!connected} onClick={onNewFolder}>
          <FolderPlusIcon className="size-3.5" />
          Folder
        </Button>
        <Button variant="outline" size="sm" disabled={!connected || !canChange} onClick={onRename}>
          <Edit3Icon className="size-3.5" />
          Rename
        </Button>
        <Button variant="outline" size="sm" disabled={!connected || !isFile} onClick={onEdit}>
          <Edit3Icon className="size-3.5" />
          Edit
        </Button>
        <Button variant="outline" size="sm" disabled={!connected || !canChange} onClick={onCopy}>
          <CopyIcon className="size-3.5" />
          Copy →
        </Button>
        <Button variant="outline" size="sm" disabled={!connected || !canChange} onClick={onMove}>
          <ArrowLeftRightIcon className="size-3.5" />
          Move →
        </Button>
        <Button
          variant="outline"
          size="sm"
          disabled={!connected || !isFile}
          onClick={onDownload}
        >
          <DownloadIcon className="size-3.5" />
          Download
        </Button>
        <Button
          variant="outline"
          size="sm"
          disabled={!connected}
          onClick={() => inputRef.current?.click()}
        >
          <UploadIcon className="size-3.5" />
          Upload
        </Button>
        <Button
          variant="outline"
          size="sm"
          className="text-destructive"
          disabled={!connected || !selected}
          onClick={onDelete}
        >
          <Trash2Icon className="size-3.5" />
          Delete
        </Button>
        <input
          ref={inputRef}
          type="file"
          multiple
          className="hidden"
          onChange={(event) => {
            if (event.target.files?.length) onUpload(event.target.files)
            event.target.value = ""
          }}
        />
        <span className="ml-auto hidden items-center gap-1 text-[10px] text-muted-foreground lg:flex">
          <ArrowDownToLineIcon className="size-3" />
          Drop from PC or the other pane
        </span>
      </div>
    </section>
  )
}
