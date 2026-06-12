// The one fetch wrapper (docs/FRONTEND_RULES.md: no raw fetch in
// components). Same-origin requests; the session cookie does auth.

import type { ErrorEnvelope } from "@/lib/types"

export class ApiError extends Error {
  readonly code: string
  readonly status: number

  constructor(status: number, code: string, message: string) {
    super(message)
    this.code = code
    this.status = status
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
    try {
      const body = (await resp.json()) as ErrorEnvelope
      code = body.error.code
      message = body.error.message
    } catch {
      // non-envelope error body; keep defaults
    }
    throw new ApiError(resp.status, code, message)
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
  serverDetail: (id: string) => ["servers", id] as const,
  serverMetrics: (id: string, minutes: number) => ["servers", id, "metrics", minutes] as const,
  serverVolumes: (id: string) => ["servers", id, "volumes"] as const,
  deployments: ["deployments"] as const,
  deploymentDetail: (id: string) => ["deployments", id] as const,
  metricsLatest: ["metrics", "latest"] as const,
}
