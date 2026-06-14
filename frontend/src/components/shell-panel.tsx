import { useCallback, useEffect, useRef, useState } from "react"
import { PowerOffIcon, TerminalIcon } from "lucide-react"
import { FitAddon } from "@xterm/addon-fit"
import { Terminal } from "@xterm/xterm"
import "@xterm/xterm/css/xterm.css"

import { DetailPanel } from "@/components/detail-panel"
import { Button } from "@/components/ui/button"

type Status = "idle" | "connecting" | "open" | "closed" | "error"

/** Interactive container shell (operator: web terminal, no SSH). On Start
 * it opens a WebSocket to `/api/deployments/{id}/shell`; the controller
 * bridges it to the agent, which `docker exec`s bash→sh with a TTY. xterm
 * renders the PTY; keystrokes go up as binary, resizes as a JSON control. */
export function ShellPanel({
  deploymentId,
  running,
  expanded,
  onToggle,
  className,
}: {
  deploymentId: string
  running: boolean
  expanded: boolean
  onToggle: () => void
  className?: string
}) {
  const [status, setStatus] = useState<Status>("idle")
  const [note, setNote] = useState<string | null>(null)
  const hostRef = useRef<HTMLDivElement>(null)
  const termRef = useRef<Terminal | null>(null)
  const fitRef = useRef<FitAddon | null>(null)
  const wsRef = useRef<WebSocket | null>(null)
  const cleanupRef = useRef<(() => void) | null>(null)

  const teardown = useCallback(() => {
    cleanupRef.current?.()
    cleanupRef.current = null
    wsRef.current = null
    termRef.current = null
    fitRef.current = null
  }, [])

  const stop = useCallback(() => {
    teardown()
    setStatus("idle")
    setNote(null)
  }, [teardown])

  const start = useCallback(() => {
    if (!hostRef.current) return
    teardown()
    setNote(null)
    setStatus("connecting")

    const dark = document.documentElement.classList.contains("dark")
    const term = new Terminal({
      cursorBlink: true,
      fontSize: 12,
      fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
      theme: dark
        ? { background: "#0a0a0a", foreground: "#e4e4e7", cursor: "#e4e4e7" }
        : { background: "#ffffff", foreground: "#18181b", cursor: "#18181b" },
    })
    const fit = new FitAddon()
    term.loadAddon(fit)
    term.open(hostRef.current)
    fit.fit()
    termRef.current = term
    fitRef.current = fit

    const proto = location.protocol === "https:" ? "wss" : "ws"
    const ws = new WebSocket(`${proto}://${location.host}/api/deployments/${deploymentId}/shell`)
    ws.binaryType = "arraybuffer"
    wsRef.current = ws
    const enc = new TextEncoder()
    const sendResize = () => {
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: "resize", cols: term.cols, rows: term.rows }))
      }
    }

    ws.onopen = () => {
      setStatus("open")
      fit.fit()
      sendResize()
      term.focus()
    }
    ws.onmessage = (e) => {
      if (e.data instanceof ArrayBuffer) term.write(new Uint8Array(e.data))
      else term.write(String(e.data))
    }
    ws.onerror = () => setStatus((s) => (s === "open" ? s : "error"))
    ws.onclose = (e) => {
      setStatus((s) => (s === "error" ? s : "closed"))
      setNote(e.reason || null)
    }

    const dataSub = term.onData((d) => {
      if (ws.readyState === WebSocket.OPEN) ws.send(enc.encode(d))
    })
    const resizeSub = term.onResize(() => sendResize())
    const ro = new ResizeObserver(() => {
      try {
        fit.fit()
      } catch {
        /* element not measurable yet */
      }
    })
    ro.observe(hostRef.current)

    cleanupRef.current = () => {
      dataSub.dispose()
      resizeSub.dispose()
      ro.disconnect()
      try {
        ws.close()
      } catch {
        /* already closing */
      }
      term.dispose()
    }
  }, [deploymentId, teardown])

  // Dispose on unmount.
  useEffect(() => () => teardown(), [teardown])

  // Refit when the split/expand layout changes.
  useEffect(() => {
    if (status !== "open") return
    const id = setTimeout(() => {
      try {
        fitRef.current?.fit()
      } catch {
        /* not measurable */
      }
    }, 60)
    return () => clearTimeout(id)
  }, [expanded, status])

  const connected = status === "open" || status === "connecting"
  const actions = connected ? (
    <Button
      variant="ghost"
      size="icon"
      className="size-7 text-slot-failed"
      onClick={stop}
      aria-label="Disconnect shell"
      title="Disconnect"
    >
      <PowerOffIcon className="size-3.5" />
    </Button>
  ) : null

  return (
    <DetailPanel
      title="Shell"
      actions={actions}
      expanded={expanded}
      onToggle={onToggle}
      className={className}
    >
      <div className="relative min-h-0 flex-1">
        {/* xterm mounts here; kept sized so fit() works behind the overlay. */}
        <div ref={hostRef} className="h-full w-full overflow-hidden rounded-md border bg-muted/40 p-1" />
        {status !== "open" ? (
          <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 rounded-md bg-card/85 text-center">
            <TerminalIcon className="size-8 text-muted-foreground" aria-hidden />
            <Button onClick={start} disabled={!running || status === "connecting"}>
              <TerminalIcon className="size-3.5" aria-hidden />
              {status === "connecting"
                ? "Connecting…"
                : status === "closed" || status === "error"
                  ? "Reconnect"
                  : "Start shell"}
            </Button>
            <p className="max-w-xs text-xs text-muted-foreground">
              {!running
                ? "The container isn't running — start it to open a shell."
                : status === "error"
                  ? "Could not connect to the shell. Try again."
                  : status === "closed"
                    ? `Session ended${note ? `: ${note}` : "."}`
                    : "Opens an interactive shell on this container (tries bash, then sh)."}
            </p>
          </div>
        ) : null}
      </div>
    </DetailPanel>
  )
}
