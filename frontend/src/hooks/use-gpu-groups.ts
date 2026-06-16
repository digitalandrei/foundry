import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { api, ApiError, queryKeys } from "@/lib/api"
import type {
  CreateGpuGroupRequest,
  GpuGroup,
  SetGroupUseModeRequest,
  SetSlotUseModeRequest,
} from "@/lib/types"

/** GPU groups defined on a server (any authed user can read; only the
 * grid/server-config render them). Group deployability is computed
 * server-side — we never recompute `deployable`/`busy_reason` here. */
export function useServerGroups(serverId: string | null) {
  return useQuery({
    queryKey: queryKeys.serverGroups(serverId ?? ""),
    queryFn: () => api<GpuGroup[]>(`/api/servers/${serverId}/gpu-groups`),
    enabled: serverId !== null,
    // Busy/deployable flips with deploy lifecycle — keep it close to live.
    refetchInterval: 10_000,
  })
}

/** Invalidate everything a group/use-mode change can move: the group
 * list, the server (slot states + per-GPU membership), and deployments. */
function invalidateGroups(
  queryClient: ReturnType<typeof useQueryClient>,
  serverId: string,
) {
  queryClient.invalidateQueries({ queryKey: queryKeys.serverGroups(serverId) })
  queryClient.invalidateQueries({ queryKey: queryKeys.serverDetail(serverId) })
  queryClient.invalidateQueries({ queryKey: queryKeys.servers })
  queryClient.invalidateQueries({ queryKey: queryKeys.deployments })
}

/** Create a GPU group (admin-only — the server rejects non-admins 403). */
export function useCreateGroup(serverId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (req: CreateGpuGroupRequest) =>
      api<GpuGroup>(`/api/servers/${serverId}/gpu-groups`, {
        method: "POST",
        body: JSON.stringify(req),
      }),
    onSuccess: (g) => {
      toast.success(`Group ${g.name} created`)
      invalidateGroups(queryClient, serverId)
    },
    onError: (err) => toast.error(err instanceof ApiError ? err.message : "Failed to create group"),
  })
}

/** Delete a group (admin-only; refused 400 while a deploy is live on it —
 * the API's message names the holder, which we surface). */
export function useDeleteGroup(serverId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (groupId: string) => api<void>(`/api/gpu-groups/${groupId}`, { method: "DELETE" }),
    onSuccess: () => {
      toast.success("Group deleted")
      invalidateGroups(queryClient, serverId)
    },
    onError: (err) => toast.error(err instanceof ApiError ? err.message : "Failed to delete group"),
  })
}

/** Set a group's concurrency cap (admin-only). 1 = single-use (one
 * exclusive container across the GPUs); >1 = multi-use soft sharing. */
export function useSetGroupUseMode(serverId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ groupId, req }: { groupId: string; req: SetGroupUseModeRequest }) =>
      api<void>(`/api/gpu-groups/${groupId}`, {
        method: "PATCH",
        body: JSON.stringify(req),
      }),
    onSuccess: (_data, { req }) => {
      toast.success(
        req.max_occupants > 1
          ? `Group set to multi-use · max ${req.max_occupants}`
          : "Group set to single-use",
      )
      invalidateGroups(queryClient, serverId)
    },
    onError: (err) => toast.error(err instanceof ApiError ? err.message : "Failed to update group"),
  })
}

/** Set a slot's concurrency cap (admin-only). 1 = single-use; >1 = soft
 * multi-use sharing with no VRAM isolation. */
export function useSetSlotUseMode(serverId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ slotId, req }: { slotId: string; req: SetSlotUseModeRequest }) =>
      api<void>(`/api/slots/${slotId}`, {
        method: "PATCH",
        body: JSON.stringify(req),
      }),
    onSuccess: (_data, { req }) => {
      toast.success(req.max_occupants > 1 ? `Slot set to multi-use · max ${req.max_occupants}` : "Slot set to single-use")
      invalidateGroups(queryClient, serverId)
    },
    onError: (err) => toast.error(err instanceof ApiError ? err.message : "Failed to update slot"),
  })
}
