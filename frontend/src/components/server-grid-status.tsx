import { CheckCircle2Icon, TriangleAlertIcon } from "lucide-react"

import type { HostAlertFlags } from "@/lib/server-alerts"
import { cn } from "@/lib/utils"

export function HostAlerts({ alerts }: { alerts: HostAlertFlags }) {
  const badges = [
    alerts.disk && {
      key: "disk",
      label: `disk ${alerts.diskPct.toFixed(0)}%`,
      title:
        "Root filesystem is nearly full — image pulls and container writes will start failing. Free space on the server.",
    },
    alerts.mem && {
      key: "mem",
      label: `mem ${alerts.memPct.toFixed(0)}%`,
      title: "Host memory is nearly exhausted — new containers risk being OOM-killed.",
    },
    alerts.cpu && {
      key: "cpu",
      label: `cpu ${alerts.cpuRatio.toFixed(1)}×`,
      title:
        "1-minute load average exceeds the logical core count — the host is CPU-saturated and work will contend.",
    },
  ].filter(Boolean) as { key: string; label: string; title: string }[]

  return badges.map((badge) => (
    <span
      key={badge.key}
      className="inline-flex items-center gap-1 text-xs text-slot-reserved"
      title={badge.title}
    >
      <TriangleAlertIcon className="size-3.5" aria-hidden />
      {badge.label}
    </span>
  ))
}

export function DockerStatus({ ok }: { ok: boolean | null }) {
  if (ok === null) return null
  if (ok) {
    return (
      <span
        className="inline-flex items-center gap-1 text-xs text-slot-free"
        title="The Docker daemon is running — deployments are allowed."
      >
        <CheckCircle2Icon className="size-3.5" aria-hidden />
        docker: active
      </span>
    )
  }
  return (
    <span
      className="inline-flex items-center gap-1 text-xs text-slot-failed"
      title="The Docker daemon is not responding on this server. Start it (`sudo systemctl start docker`); deploys are blocked until it's back."
    >
      <TriangleAlertIcon className="size-3.5" aria-hidden />
      Docker stopped — deploys blocked
    </span>
  )
}

export function NginxStatus({ status, ready }: { status: string | null; ready: boolean | null }) {
  if (status === null && ready === null) return null
  if (status === "READY" || (status === null && ready === true)) {
    return (
      <span
        className="inline-flex items-center gap-1 text-xs text-slot-free"
        title="nginx is active and configured — HTTP/S app publishing is ready."
      >
        <CheckCircle2Icon className="size-3.5" aria-hidden />
        nginx: active
      </span>
    )
  }
  const warn: Record<string, { label: string; title: string }> = {
    NGINX_MISSING: {
      label: "nginx not installed — HTTP/S off",
      title: "Install nginx, then run `sudo foundry-agent --setup-apps`.",
    },
    NGINX_OUTDATED: {
      label: "nginx too old — HTTP/S off",
      title: "nginx is older than 1.25.1. Upgrade nginx on the server.",
    },
    NGINX_INACTIVE: {
      label: "nginx stopped — HTTP/S off",
      title: "nginx is installed but not running. `sudo systemctl enable --now nginx`.",
    },
    NOT_CONFIGURED: {
      label: "nginx up · not configured — HTTP/S off",
      title: "Run `sudo foundry-agent --setup-apps` on the server.",
    },
    TLS_MISSING: {
      label: "TLS cert missing — HTTP/S off",
      title: "Install fullchain.pem + privkey.pem under /etc/foundry-agent/tls/.",
    },
  }
  const warning = (status && warn[status]) || {
    label: "HTTP/S publishing off",
    title: "The agent reports HTTP/S app publishing is unavailable on this server.",
  }
  return (
    <span
      className="inline-flex items-center gap-1 text-xs text-slot-reserved"
      title={warning.title}
    >
      <TriangleAlertIcon className="size-3.5" aria-hidden />
      {warning.label}
    </span>
  )
}

export function StatusDot({ running, className }: { running: boolean; className?: string }) {
  return (
    <span
      className={cn(
        "size-2 shrink-0 rounded-full",
        className ?? (running ? "bg-slot-running" : "bg-slot-offline"),
      )}
      title={running ? "running" : "stopped"}
      aria-hidden
    />
  )
}
