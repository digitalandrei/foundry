// The one fetch wrapper (docs/FRONTEND_RULES.md: no raw fetch in
// components). Same-origin requests; the session cookie does auth.

import type { ErrorEnvelope } from "@/lib/types"

export class ApiError extends Error {
  readonly code: string
  readonly status: number
  readonly details: unknown

  constructor(status: number, code: string, message: string, details?: unknown) {
    super(message)
    this.code = code
    this.status = status
    this.details = details
  }
}

export async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const resp = await fetch(path, {
    credentials: "same-origin",
    headers: init?.body ? { "Content-Type": "application/json" } : undefined,
    ...init,
  })

  if (!resp.ok) {
    let code = "unknown"
    let message = `request failed (${resp.status})`
    let details: unknown
    try {
      const body = (await resp.json()) as ErrorEnvelope
      code = body.error.code
      message = body.error.message
      details = body.error.details
    } catch {
      // non-envelope error body; keep defaults
    }
    throw new ApiError(resp.status, code, message, details)
  }

  if (resp.status === 204) {
    return undefined as T
  }
  return (await resp.json()) as T
}

// Centralized query-key factory (docs/FRONTEND_RULES.md).
export const queryKeys = {
  me: ["me"] as const,
  instances: ["instances"] as const,
  instancesFull: ["instances", "full"] as const,
  projects: ["projects"] as const,
  registry: (projectId: string) => ["registry", projectId] as const,
  exposedPorts: (tagId: string) => ["registry", "tags", tagId, "exposed-ports"] as const,
  servers: ["servers"] as const,
  fleetTokens: ["fleet-tokens"] as const,
  serverDetail: (id: string) => ["servers", id] as const,
  serverMetrics: (id: string, minutes: number) => ["servers", id, "metrics", minutes] as const,
  serverVolumes: (id: string) => ["servers", id, "volumes"] as const,
  serverGroups: (id: string) => ["servers", id, "gpu-groups"] as const,
  deployments: ["deployments"] as const,
  deploymentDetail: (id: string) => ["deployments", id] as const,
  deploymentLogs: (id: string) => ["deployments", id, "logs"] as const,
  metricsLatest: ["metrics", "latest"] as const,
  audit: (action: string | null) => ["audit", action ?? "all"] as const,
  registryUpdates: ["registry", "updates"] as const,
}
