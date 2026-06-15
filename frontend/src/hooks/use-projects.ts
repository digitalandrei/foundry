import { useQuery } from "@tanstack/react-query"

import { api, queryKeys } from "@/lib/api"
import type { ProjectSummary, RegistryBrowseResponse, RegistryUpdates } from "@/lib/types"

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

/** New-image poller — a cheap, name-only registry sync across the user's
 * available repos. The watcher provider turns each response into toasts +
 * sidebar badges. Cost note: each poll lists repos+tag-names for every
 * available project, so keep the interval gentle. */
export function useRegistryUpdates() {
  return useQuery({
    queryKey: queryKeys.registryUpdates,
    queryFn: () => api<RegistryUpdates>("/api/registry/updates"),
    refetchInterval: 90_000,
    staleTime: 60_000,
  })
}
