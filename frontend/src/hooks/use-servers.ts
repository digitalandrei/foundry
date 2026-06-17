import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { api, ApiError, queryKeys } from "@/lib/api"
import type {
  EnrollmentTokenResponse,
  FleetTokenResponse,
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

/** Mint a reusable fleet enrollment key (admin only). Not server-scoped,
 * so it does not invalidate the server list. */
export function useCreateFleetToken() {
  return useMutation({
    mutationFn: (req: { ttl_hours: number; max_uses: number | null }) =>
      api<FleetTokenResponse>("/api/fleet-tokens", {
        method: "POST",
        body: JSON.stringify(req),
      }),
    onError: (err) => {
      toast.error(err instanceof ApiError ? err.message : "Failed to mint fleet key")
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
