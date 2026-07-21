export type FileEntryKind = "directory" | "file" | "symlink"

export interface VolumeFileEntry {
  name: string
  path: string
  kind: FileEntryKind
  size: number
  modified_at: string | null
}

export type FileServerMessage =
  | { type: "listing"; request_id: string; path: string; entries: VolumeFileEntry[] }
  | { type: "text"; request_id: string; content: string }
  | { type: "upload_ready"; request_id: string; offset: number }
  | { type: "download_start"; request_id: string; name: string; size: number }
  | { type: "download_chunk"; request_id: string; data: string }
  | { type: "download_finish"; request_id: string }
  | { type: "ack"; request_id: string }
  | { type: "error"; request_id: string; message: string }

export function joinVolumePath(directory: string, name: string): string {
  return directory ? `${directory}/${name}` : name
}

export function parentVolumePath(path: string): string {
  const index = path.lastIndexOf("/")
  return index < 0 ? "" : path.slice(0, index)
}

export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KiB`
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MiB`
  return `${(bytes / 1024 ** 3).toFixed(1)} GiB`
}

export function agentSupportsVolumeFiles(version: string | null): boolean {
  return agentVersionAtLeast(version, [0, 56, 0])
}

export function bytesToBase64(bytes: Uint8Array): string {
  let binary = ""
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000))
  }
  return btoa(binary)
}

export function base64ToBytes(value: string): Uint8Array {
  const binary = atob(value)
  const bytes = new Uint8Array(binary.length)
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index)
  }
  return bytes
}
import { agentVersionAtLeast } from "@/lib/agent-version"
