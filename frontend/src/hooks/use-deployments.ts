import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { api, ApiError, queryKeys } from "@/lib/api"
import type {
  CreateDeploymentRequest,
  DeploymentDetail,
  DeploymentLogsView,
  DeploymentSummary,
  ImageMetadataResponse,
  LatestMetricsResponse,
  AppRequestMetrics,
  AppTrafficRecord,
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

export function useAppTraffic(id: string | null) {
  const logs = useQuery({
    queryKey: ["deployments", id, "access-logs"],
    queryFn: () => api<AppTrafficRecord[]>(`/api/deployments/${id}/access-logs`),
    enabled: id !== null,
    refetchInterval: 5_000,
  })
  const metrics = useQuery({
    queryKey: ["deployments", id, "request-metrics"],
    queryFn: () => api<AppRequestMetrics>(`/api/deployments/${id}/request-metrics`),
    enabled: id !== null,
    refetchInterval: 10_000,
  })
  return { logs, metrics }
}

/** Mounts + env names for the slot detail dialog. */
export function useDeploymentDetail(id: string | null) {
  return useQuery({
    queryKey: queryKeys.deploymentDetail(id ?? ""),
    queryFn: () => api<DeploymentDetail>(`/api/deployments/${id}`),
    enabled: id !== null,
    refetchInterval: 5_000, // live state/progress while the dialog is open
  })
}

/** Captured stdout+stderr for a deployment (deployment detail viewer).
 * Follow mode polls; paused mode fetches once and on manual refresh. */
export function useDeploymentLogs(id: string | null, follow: boolean) {
  return useQuery({
    queryKey: queryKeys.deploymentLogs(id ?? ""),
    queryFn: () => api<DeploymentLogsView>(`/api/deployments/${id}/logs`),
    enabled: id !== null,
    refetchInterval: follow ? 3_000 : false,
  })
}

/** Newest telemetry sample per server — live slot-grid labels. */
export function useLatestMetrics() {
  return useQuery({
    queryKey: queryKeys.metricsLatest,
    queryFn: () => api<LatestMetricsResponse>("/api/metrics/latest"),
    refetchInterval: 15_000, // agents sample every 30s
  })
}

/** Ports, persistent mounts, and size discovered from image metadata.
 * The server degrades discovery failures to empty editable defaults. */
export function useImageMetadata(tagId: string | null) {
  return useQuery({
    queryKey: queryKeys.imageMetadata(tagId ?? ""),
    queryFn: () => api<ImageMetadataResponse>(`/api/registry/tags/${tagId}/metadata`),
    enabled: tagId !== null,
    staleTime: 5 * 60_000, // image config is immutable per tag push
  })
}

/** Volumes the requester may mount on a server (deploy dialog reuse). */
export function useServerVolumes(
  serverId: string | null,
  projectId: string | null,
  target?: { slotId?: string; groupId?: string },
) {
  const targetKey = target?.slotId ? `slot:${target.slotId}` : target?.groupId ? `group:${target.groupId}` : "all"
  const query = new URLSearchParams()
  if (projectId) query.set("project_id", projectId)
  if (target?.slotId) query.set("slot_id", target.slotId)
  if (target?.groupId) query.set("gpu_group_id", target.groupId)
  return useQuery({
    queryKey: queryKeys.serverVolumes(serverId ?? "", projectId ?? "", targetKey),
    queryFn: () => api<ServerVolume[]>(`/api/servers/${serverId}/volumes?${query}`),
    enabled: serverId !== null && projectId !== null,
  })
}

export function useCleanVolume() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) =>
      api<void>(`/api/volumes/${id}/clean`, { method: "POST" }),
    onSuccess: () => {
      toast.success("Volume clean queued")
      queryClient.invalidateQueries({ queryKey: ["servers"] })
    },
    onError: (err) => toast.error(err instanceof ApiError ? err.message : "Clean failed"),
  })
}

export function useDeleteVolume() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) =>
      api<void>(`/api/volumes/${id}`, { method: "DELETE" }),
    onSuccess: () => {
      toast.success("Volume deletion queued")
      queryClient.invalidateQueries({ queryKey: ["servers"] })
    },
    onError: (err) => toast.error(err instanceof ApiError ? err.message : "Delete failed"),
  })
}

export function useSetVolumeQuota() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, quotaBytes }: { id: string; quotaBytes: number | null }) =>
      api<void>(`/api/volumes/${id}`, {
        method: "PATCH",
        body: JSON.stringify({ quota_bytes: quotaBytes }),
      }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["servers"] }),
    onError: (err) => toast.error(err instanceof ApiError ? err.message : "Quota update failed"),
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
      const where = d.gpu_group_id ? `group ${d.group_name}` : `slot ${d.slot_name}`
      toast.success(`Deploying ${d.name} to ${d.server_name} · ${where}`)
      invalidateAll(queryClient)
    },
    onError: (err) => {
      toast.error(err instanceof ApiError ? err.message : "Deployment failed")
    },
  })
}

/** Adopt an externally-created container into a managed deployment so it
 * gets Foundry's control surface (admin only). */
export function useAdoptContainer() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ serverId, containerId }: { serverId: string; containerId: string }) =>
      api<DeploymentSummary>(
        `/api/servers/${serverId}/containers/${containerId}/adopt`,
        { method: "POST" },
      ),
    onSuccess: (d) => {
      toast.success(`Adopted ${d.name} — now controllable like a deployment`)
      invalidateAll(queryClient)
    },
    onError: (err) => {
      toast.error(err instanceof ApiError ? err.message : "Could not adopt container")
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

export function useRetryPublish() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => api<DeploymentSummary>(`/api/deployments/${id}/retry-publish`, { method: "POST" }),
    onSuccess: () => {
      toast.success("Publication retry queued")
      invalidateAll(queryClient)
    },
    onError: (err) => toast.error(err instanceof ApiError ? err.message : "Publication retry failed"),
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

/** Clear a FAILED deployment and free its stuck slot (no agent). */
export function useDismissDeployment() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) =>
      api<void>(`/api/deployments/${id}/dismiss`, { method: "POST" }),
    onSuccess: () => {
      toast.success("Failed deployment cleared — slot released")
      invalidateAll(queryClient)
    },
    onError: (err) => toast.error(err instanceof ApiError ? err.message : "Dismiss failed"),
  })
}
