import { useCallback, useEffect, useRef, useState } from "react"

import {
  base64ToBytes,
  bytesToBase64,
  type FileServerMessage,
  type VolumeFileEntry,
} from "@/lib/volume-files"

type ConnectionStatus = "idle" | "connecting" | "open" | "closed" | "error"
type SingleResult =
  | { type: "listing"; path: string; entries: VolumeFileEntry[] }
  | { type: "text"; content: string }
  | { type: "ack" }

type Pending = {
  resolve: (result: SingleResult | Blob) => void
  reject: (error: Error) => void
  ready?: (offset: number) => void
  readyReject?: (error: Error) => void
  downloadChunks?: ArrayBuffer[]
}

function requestId() {
  return crypto.randomUUID()
}

/** One reverse-WS file session shared by both MC-style panes. Server state
 * remains on the agent; this hook only correlates bounded JSON requests and
 * streams upload/download chunks. */
export function useVolumeFiles(
  serverId: string | null,
  projectId: string | null,
  enabled: boolean,
) {
  const [status, setStatus] = useState<ConnectionStatus>("idle")
  const [note, setNote] = useState<string | null>(null)
  const wsRef = useRef<WebSocket | null>(null)
  const pendingRef = useRef(new Map<string, Pending>())

  useEffect(() => {
    for (const pending of pendingRef.current.values()) {
      pending.reject(new Error("file session changed"))
    }
    pendingRef.current.clear()
    wsRef.current?.close()
    wsRef.current = null
    if (!enabled || !serverId || !projectId) {
      queueMicrotask(() => {
        if (wsRef.current === null) setStatus("idle")
      })
      return
    }

    const protocol = location.protocol === "https:" ? "wss" : "ws"
    const query = new URLSearchParams({ project_id: projectId })
    const ws = new WebSocket(
      `${protocol}://${location.host}/api/servers/${serverId}/volume-files?${query}`,
    )
    wsRef.current = ws
    queueMicrotask(() => {
      if (wsRef.current === ws) {
        setNote(null)
        setStatus("connecting")
      }
    })
    ws.onopen = () => {
      if (wsRef.current === ws) setStatus("open")
    }
    ws.onerror = () => {
      if (wsRef.current === ws) setStatus("error")
    }
    ws.onclose = (event) => {
      if (wsRef.current !== ws) return
      setStatus((current) => (current === "error" ? current : "closed"))
      setNote(event.reason || "The file session ended.")
      for (const pending of pendingRef.current.values()) {
        pending.reject(new Error(event.reason || "file session ended"))
      }
      pendingRef.current.clear()
    }
    ws.onmessage = (event) => {
      if (typeof event.data !== "string") return
      let message: FileServerMessage
      try {
        message = JSON.parse(event.data) as FileServerMessage
      } catch {
        return
      }
      const pending = pendingRef.current.get(message.request_id)
      if (!pending) return
      if (message.type === "error") {
        pendingRef.current.delete(message.request_id)
        const error = new Error(message.message)
        pending.readyReject?.(error)
        pending.reject(error)
      } else if (message.type === "listing") {
        pendingRef.current.delete(message.request_id)
        pending.resolve({ type: "listing", path: message.path, entries: message.entries })
      } else if (message.type === "text") {
        pendingRef.current.delete(message.request_id)
        pending.resolve({ type: "text", content: message.content })
      } else if (message.type === "ack") {
        pendingRef.current.delete(message.request_id)
        pending.resolve({ type: "ack" })
      } else if (message.type === "upload_ready") {
        pending.ready?.(message.offset)
      } else if (message.type === "download_start") {
        pending.downloadChunks = []
      } else if (message.type === "download_chunk") {
        pending.downloadChunks?.push(base64ToBytes(message.data).buffer as ArrayBuffer)
      } else if (message.type === "download_finish") {
        pendingRef.current.delete(message.request_id)
        pending.resolve(
          new Blob(pending.downloadChunks ?? [], { type: "application/octet-stream" }),
        )
      }
    }
    return () => {
      ws.close()
      wsRef.current = null
    }
  }, [enabled, projectId, serverId])

  const send = useCallback((message: Record<string, unknown>) => {
    const ws = wsRef.current
    if (!ws || ws.readyState !== WebSocket.OPEN) {
      return Promise.reject(new Error("volume file session is not connected"))
    }
    const id = message.request_id as string
    return new Promise<SingleResult | Blob>((resolve, reject) => {
      pendingRef.current.set(id, { resolve, reject })
      ws.send(JSON.stringify(message))
    })
  }, [])

  const list = useCallback(
    async (volumeId: string, path: string) => {
      const result = await send({
        type: "list",
        request_id: requestId(),
        volume_id: volumeId,
        path,
      })
      if (!(result instanceof Blob) && result.type === "listing") return result.entries
      throw new Error("unexpected listing response")
    },
    [send],
  )

  const readText = useCallback(
    async (volumeId: string, path: string) => {
      const result = await send({
        type: "read_text",
        request_id: requestId(),
        volume_id: volumeId,
        path,
      })
      if (!(result instanceof Blob) && result.type === "text") return result.content
      throw new Error("unexpected text response")
    },
    [send],
  )

  const mutate = useCallback(
    async (message: Record<string, unknown>) => {
      const result = await send({ ...message, request_id: requestId() })
      if (!(result instanceof Blob) && result.type === "ack") return
      throw new Error("unexpected operation response")
    },
    [send],
  )

  const download = useCallback(
    async (volumeId: string, path: string, fallbackName: string) => {
      const id = requestId()
      const ws = wsRef.current
      if (!ws || ws.readyState !== WebSocket.OPEN) {
        throw new Error("volume file session is not connected")
      }
      const blob = await new Promise<Blob>((resolve, reject) => {
        pendingRef.current.set(id, {
          resolve: (result) =>
            result instanceof Blob ? resolve(result) : reject(new Error("unexpected download response")),
          reject,
          downloadChunks: [],
        })
        ws.send(
          JSON.stringify({
            type: "download",
            request_id: id,
            volume_id: volumeId,
            path,
          }),
        )
      })
      const pendingName = fallbackName
      const url = URL.createObjectURL(blob)
      const anchor = document.createElement("a")
      anchor.href = url
      anchor.download = pendingName
      anchor.click()
      setTimeout(() => URL.revokeObjectURL(url), 10_000)
    },
    [],
  )

  const upload = useCallback(async (volumeId: string, path: string, file: File) => {
    const ws = wsRef.current
    if (!ws || ws.readyState !== WebSocket.OPEN) {
      throw new Error("volume file session is not connected")
    }
    const resumeKey = `foundry-upload:${serverId}:${projectId}:${volumeId}:${path}:${file.name}:${file.size}:${file.lastModified}`
    const id = localStorage.getItem(resumeKey) ?? requestId()
    localStorage.setItem(resumeKey, id)
    let markReady: ((offset: number) => void) | undefined
    let rejectReady: ((error: Error) => void) | undefined
    const ready = new Promise<number>((resolve, reject) => {
      markReady = resolve
      rejectReady = reject
    })
    const finished = new Promise<void>((resolve, reject) => {
      pendingRef.current.set(id, {
        resolve: (result) =>
          !(result instanceof Blob) && result.type === "ack"
            ? resolve()
            : reject(new Error("unexpected upload response")),
        reject,
        ready: markReady,
        readyReject: rejectReady,
      })
    })
    // If the agent rejects UploadStart, both readiness and completion
    // reject; completion is awaited after streaming, so mark it handled now.
    void finished.catch(() => undefined)
    ws.send(
      JSON.stringify({
        type: "upload_start",
        request_id: id,
        volume_id: volumeId,
        path,
        size: file.size,
      }),
    )
    const resumeOffset = await ready
    if (resumeOffset > file.size) throw new Error("server upload offset exceeds this file")
    const chunkSize = 128 * 1024
    for (let offset = resumeOffset; offset < file.size; offset += chunkSize) {
      while (ws.bufferedAmount > 4 * 1024 * 1024) {
        await new Promise((resolve) => setTimeout(resolve, 20))
      }
      const bytes = new Uint8Array(await file.slice(offset, offset + chunkSize).arrayBuffer())
      ws.send(
        JSON.stringify({
          type: "upload_chunk",
          request_id: id,
          offset,
          data: bytesToBase64(bytes),
        }),
      )
    }
    ws.send(JSON.stringify({ type: "upload_finish", request_id: id }))
    await finished
    localStorage.removeItem(resumeKey)
  }, [projectId, serverId])

  return {
    status,
    note,
    list,
    readText,
    writeText: (volumeId: string, path: string, content: string) =>
      mutate({ type: "write_text", volume_id: volumeId, path, content }),
    mkdir: (volumeId: string, path: string) =>
      mutate({ type: "mkdir", volume_id: volumeId, path }),
    rename: (volumeId: string, from: string, to: string) =>
      mutate({ type: "rename", volume_id: volumeId, from, to }),
    copy: (
      fromVolumeId: string,
      from: string,
      toVolumeId: string,
      to: string,
    ) =>
      mutate({
        type: "copy",
        from_volume_id: fromVolumeId,
        from,
        to_volume_id: toVolumeId,
        to,
      }),
    move: (
      fromVolumeId: string,
      from: string,
      toVolumeId: string,
      to: string,
    ) =>
      mutate({
        type: "move",
        from_volume_id: fromVolumeId,
        from,
        to_volume_id: toVolumeId,
        to,
      }),
    remove: (volumeId: string, path: string) =>
      mutate({ type: "delete", volume_id: volumeId, path }),
    download,
    upload,
  }
}
