import type { HostMetrics } from "@/lib/types"

const DISK_WARN_PCT = 90
const MEM_WARN_PCT = 90
const CPU_WARN_RATIO = 1.0

export function hostAlerts(host: HostMetrics) {
  const diskPct = (host.disk_used_gb / Math.max(1, host.disk_total_gb)) * 100
  const memPct = (host.mem_used_mb / Math.max(1, host.mem_total_mb)) * 100
  const cpuRatio = host.load_avg_1m / Math.max(1, host.cpu_cores)
  return {
    disk: diskPct >= DISK_WARN_PCT,
    mem: memPct >= MEM_WARN_PCT,
    cpu: cpuRatio >= CPU_WARN_RATIO,
    diskPct,
    memPct,
    cpuRatio,
  }
}

export type HostAlertFlags = ReturnType<typeof hostAlerts>
