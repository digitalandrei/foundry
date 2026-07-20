import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { api, ApiError, queryKeys } from "@/lib/api"
import type {
  EnrollmentTokenResponse,
  FleetTokenResponse,
  FleetTokenSummary,
  MetricsPoint,
  ServerDetail,
  ServerSummary,
} from "@/lib/types"

export function useServers() {
  return useQuery({
    queryKey: queryKeys.servers,
    queryFn: () => api<ServerSummary[]>("/api/servers"),
    // Heartbeats land every ~15s; keep the fleet view close to live.
    refetchInterval: 10_000,
  })
}

/** Detail (docker ps + GPU breakdown); fetched while the dialog is open. */
export function useServerDetail(serverId: string | null) {
  return useQuery({
    queryKey: queryKeys.serverDetail(serverId ?? ""),
    queryFn: () => api<ServerDetail>(`/api/servers/${serverId}`),
    enabled: serverId !== null,
    refetchInterval: 15_000,
  })
}

/** Telemetry series for the server page (30s samples, 24h retention). */
export function useServerMetrics(serverId: string, minutes = 60) {
  return useQuery({
    queryKey: queryKeys.serverMetrics(serverId, minutes),
    queryFn: () => api<MetricsPoint[]>(`/api/servers/${serverId}/metrics?minutes=${minutes}`),
    refetchInterval: 30_000,
  })
}

export function useCreateServer() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (name: string) =>
      api<EnrollmentTokenResponse>("/api/servers", {
        method: "POST",
        body: JSON.stringify({ name }),
      }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.servers }),
    onError: (err) => {
      toast.error(err instanceof ApiError ? err.message : "Failed to create server")
    },
  })
}

/** Live fleet enrollment keys (admin). Many may coexist. */
export function useFleetTokens(enabled: boolean) {
  return useQuery({
    queryKey: queryKeys.fleetTokens,
    queryFn: () => api<FleetTokenSummary[]>("/api/fleet-tokens"),
    enabled,
  })
}

/** Mint a reusable fleet enrollment key (admin only). */
export function useCreateFleetToken() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (req: { ttl_hours: number; max_uses: number | null }) =>
      api<FleetTokenResponse>("/api/fleet-tokens", {
        method: "POST",
        body: JSON.stringify(req),
      }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.fleetTokens }),
    onError: (err) => {
      toast.error(err instanceof ApiError ? err.message : "Failed to mint fleet key")
    },
  })
}

/** Delete (revoke) a fleet key — works even before it expires. */
export function useDeleteFleetToken() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) =>
      api<void>(`/api/fleet-tokens/${id}`, { method: "DELETE" }),
    onSuccess: () => {
      toast.success("Fleet key deleted")
      queryClient.invalidateQueries({ queryKey: queryKeys.fleetTokens })
    },
    onError: (err) => {
      toast.error(err instanceof ApiError ? err.message : "Failed to delete fleet key")
    },
  })
}

export function useRegenerateToken() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (serverId: string) =>
      api<EnrollmentTokenResponse>(`/api/servers/${serverId}/enrollment-token`, {
        method: "POST",
      }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.servers }),
    onError: (err) => {
      toast.error(err instanceof ApiError ? err.message : "Failed to generate token")
    },
  })
}

/** Hard-delete a server only when the controller confirms it has no durable
 * workload history. A 409 names the blocking dependency counts. */
export function useDeleteServer() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (serverId: string) =>
      api<void>(`/api/servers/${serverId}`, { method: "DELETE" }),
    onSuccess: () => {
      toast.success("Unused server deleted")
      queryClient.invalidateQueries({ queryKey: queryKeys.servers })
    },
    onError: (err) => {
      if (err instanceof ApiError && err.status === 409) {
        const dependencies = (err.details as { dependencies?: Record<string, number> } | undefined)
          ?.dependencies
        const blockers = Object.entries(dependencies ?? {})
          .filter(([, count]) => count > 0)
          .map(([kind, count]) => `${count} ${kind.replaceAll("_", " ")}`)
        toast.error(blockers.length ? `Cannot delete: ${blockers.join(", ")}` : err.message)
      } else {
        toast.error(err instanceof ApiError ? err.message : "Failed to delete server")
      }
    },
  })
}
