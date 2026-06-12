import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { api, ApiError, queryKeys } from "@/lib/api"
import type {
  CreateInstanceRequest,
  InstanceAdmin,
  InstancePublic,
  UpdateInstanceRequest,
} from "@/lib/types"

/** Public list for the login picker (the one unauthenticated call). */
export function useInstances() {
  return useQuery({
    queryKey: queryKeys.instances,
    queryFn: () => api<InstancePublic[]>("/api/instances"),
  })
}

export function useInstancesFull(enabled: boolean) {
  return useQuery({
    queryKey: queryKeys.instancesFull,
    queryFn: () => api<InstanceAdmin[]>("/api/instances/full"),
    enabled,
  })
}

function useInstanceInvalidate() {
  const queryClient = useQueryClient()
  return () => {
    queryClient.invalidateQueries({ queryKey: queryKeys.instances })
    queryClient.invalidateQueries({ queryKey: queryKeys.instancesFull })
  }
}

export function useCreateInstance() {
  const invalidate = useInstanceInvalidate()
  return useMutation({
    mutationFn: (req: CreateInstanceRequest) =>
      api<InstanceAdmin>("/api/instances", {
        method: "POST",
        body: JSON.stringify(req),
      }),
    onSuccess: (created) => {
      toast.success(`GitLab instance "${created.name}" onboarded`)
      invalidate()
    },
    onError: (err) => {
      toast.error(err instanceof ApiError ? err.message : "Failed to onboard instance")
    },
  })
}

export function useUpdateInstance() {
  const invalidate = useInstanceInvalidate()
  return useMutation({
    mutationFn: ({ id, req }: { id: string; req: UpdateInstanceRequest }) =>
      api<InstanceAdmin>(`/api/instances/${id}`, {
        method: "PUT",
        body: JSON.stringify(req),
      }),
    onSuccess: (updated) => {
      toast.success(`GitLab instance "${updated.name}" updated`)
      invalidate()
    },
    onError: (err) => {
      toast.error(err instanceof ApiError ? err.message : "Failed to update instance")
    },
  })
}

export function useDeleteInstance() {
  const invalidate = useInstanceInvalidate()
  return useMutation({
    mutationFn: (id: string) => api<void>(`/api/instances/${id}`, { method: "DELETE" }),
    onSuccess: () => {
      toast.success("GitLab instance removed")
      invalidate()
    },
    onError: (err) => {
      toast.error(err instanceof ApiError ? err.message : "Failed to remove instance")
    },
  })
}
