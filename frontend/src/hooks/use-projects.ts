import { useQuery } from "@tanstack/react-query"

import { api, queryKeys } from "@/lib/api"
import type { ProjectSummary, RegistryBrowseResponse } from "@/lib/types"

export function useProjects(enabled = true) {
  return useQuery({
    queryKey: queryKeys.projects,
    queryFn: () => api<ProjectSummary[]>("/api/projects"),
    staleTime: 120_000,
    enabled,
  })
}

/** Registry tree of one project — fetched lazily when expanded. */
export function useRegistry(projectId: string, enabled: boolean) {
  return useQuery({
    queryKey: queryKeys.registry(projectId),
    queryFn: () => api<RegistryBrowseResponse>(`/api/registry/${projectId}`),
    staleTime: 120_000,
    enabled,
  })
}
