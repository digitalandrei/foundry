import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { api, ApiError, queryKeys } from "@/lib/api"
import type { EnrollmentTokenResponse, ServerSummary } from "@/lib/types"

export function useServers() {
  return useQuery({
    queryKey: queryKeys.servers,
    queryFn: () => api<ServerSummary[]>("/api/servers"),
    // Heartbeats land every ~15s; keep the fleet view close to live.
    refetchInterval: 10_000,
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
