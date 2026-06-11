import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { api, ApiError, queryKeys } from "@/lib/api"
import type { CreateInstanceRequest, InstanceAdmin, InstancePublic } from "@/lib/types"

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

export function useCreateInstance() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (req: CreateInstanceRequest) =>
      api<InstanceAdmin>("/api/instances", {
        method: "POST",
        body: JSON.stringify(req),
      }),
    onSuccess: (created) => {
      toast.success(`GitLab instance "${created.name}" onboarded`)
      queryClient.invalidateQueries({ queryKey: queryKeys.instances })
    },
    onError: (err) => {
      toast.error(err instanceof ApiError ? err.message : "Failed to onboard instance")
    },
  })
}
