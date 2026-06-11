import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { api, ApiError, queryKeys } from "@/lib/api"
import type { MeResponse } from "@/lib/types"

/** Session probe: 401 → null (logged out), anything else surfaces. */
export function useMe() {
  return useQuery({
    queryKey: queryKeys.me,
    queryFn: async (): Promise<MeResponse | null> => {
      try {
        return await api<MeResponse>("/api/me")
      } catch (err) {
        if (err instanceof ApiError && err.status === 401) {
          return null
        }
        throw err
      }
    },
    staleTime: 60_000,
    retry: 1,
  })
}

export function useLogout() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: () => api<void>("/auth/logout", { method: "POST" }),
    onSettled: () => {
      // Drop everything user-scoped and re-probe the session.
      queryClient.clear()
      window.location.assign("/login")
    },
  })
}
