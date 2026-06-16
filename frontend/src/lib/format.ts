/** Relative time for "last seen" cells ("12s ago", "3m ago"). */
export function formatRelative(iso: string): string {
  const seconds = Math.max(0, Math.floor((Date.now() - new Date(iso).getTime()) / 1000))
  if (seconds < 60) return `${seconds}s ago`
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`
  return `${Math.floor(seconds / 86400)}d ago`
}

/** "load / cores" — host load average (or a container's core-equivalent
 * CPU usage) over the cores available. Cores is optional so pre-upgrade
 * samples (which predate the field) degrade to just the load number. */
export function formatLoad(load: number, cores?: number): string {
  return cores == null ? load.toFixed(2) : `${load.toFixed(2)} / ${cores}`
}

/** Memory "used / max GB" — shared by host and per-container readouts. */
export function formatMemGb(usedMb: number, maxMb: number): string {
  return `${(usedMb / 1024).toFixed(1)} / ${(maxMb / 1024).toFixed(0)} GB`
}

/** Human-readable byte size, matching the mockup ("2.8 GB"). */
export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  const units = ["KB", "MB", "GB", "TB"]
  let value = bytes
  let unit = "B"
  for (const u of units) {
    if (value < 1024) break
    value /= 1024
    unit = u
  }
  return `${value.toFixed(value >= 10 ? 0 : 1)} ${unit}`
}
