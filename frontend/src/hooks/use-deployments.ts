import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { api, ApiError, queryKeys } from "@/lib/api"
import type {
  CreateDeploymentRequest,
  DeploymentSummary,
  ExposedPortsResponse,
  ServerVolume,
} from "@/lib/types"

export function useDeployments() {
  return useQuery({
    queryKey: queryKeys.deployments,
    queryFn: () => api<DeploymentSummary[]>("/api/deployments"),
    // Lifecycle moves fast (pull → run); keep the table close to live.
    refetchInterval: 5_000,
  })
}

/** EXPOSE'd ports discovered from the image config — deploy-dialog
 * prefill. The server degrades discovery failures to an empty list. */
export function useExposedPorts(tagId: string | null) {
  return useQuery({
    queryKey: queryKeys.exposedPorts(tagId ?? ""),
    queryFn: () => api<ExposedPortsResponse>(`/api/registry/tags/${tagId}/exposed-ports`),
    enabled: tagId !== null,
    staleTime: 5 * 60_000, // image config is immutable per tag push
  })
}

/** Volumes the requester may mount on a server (deploy dialog reuse). */
export function useServerVolumes(serverId: string | null) {
  return useQuery({
    queryKey: queryKeys.serverVolumes(serverId ?? ""),
    queryFn: () => api<ServerVolume[]>(`/api/servers/${serverId}/volumes`),
    enabled: serverId !== null,
  })
}

function invalidateAll(queryClient: ReturnType<typeof useQueryClient>) {
  queryClient.invalidateQueries({ queryKey: queryKeys.deployments })
  queryClient.invalidateQueries({ queryKey: queryKeys.servers })
}

export function useCreateDeployment() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (req: CreateDeploymentRequest) =>
      api<DeploymentSummary>("/api/deployments", {
        method: "POST",
        body: JSON.stringify(req),
      }),
    onSuccess: (d) => {
      toast.success(`Deploying ${d.name} to ${d.server_name} · slot ${d.slot_name}`)
      invalidateAll(queryClient)
    },
    onError: (err) => {
      toast.error(err instanceof ApiError ? err.message : "Deployment failed")
    },
  })
}

export function useReplaceDeployment() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ oldId, req }: { oldId: string; req: CreateDeploymentRequest }) =>
      api<DeploymentSummary>(`/api/deployments/${oldId}/replace`, {
        method: "POST",
        body: JSON.stringify(req),
      }),
    onSuccess: (d) => {
      toast.success(`Replacing with ${d.name} — old container stops first`)
      invalidateAll(queryClient)
    },
    onError: (err) => {
      toast.error(err instanceof ApiError ? err.message : "Replacement failed")
    },
  })
}

export function useStopDeployment() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) =>
      api<DeploymentSummary>(`/api/deployments/${id}/stop`, { method: "POST" }),
    onSuccess: () => invalidateAll(queryClient),
    onError: (err) => toast.error(err instanceof ApiError ? err.message : "Stop failed"),
  })
}

export function useRestartDeployment() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) =>
      api<DeploymentSummary>(`/api/deployments/${id}/restart`, { method: "POST" }),
    onSuccess: () => invalidateAll(queryClient),
    onError: (err) => toast.error(err instanceof ApiError ? err.message : "Restart failed"),
  })
}

export function useRemoveDeployment() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) =>
      api<DeploymentSummary>(`/api/deployments/${id}`, { method: "DELETE" }),
    onSuccess: () => invalidateAll(queryClient),
    onError: (err) => toast.error(err instanceof ApiError ? err.message : "Remove failed"),
  })
}
