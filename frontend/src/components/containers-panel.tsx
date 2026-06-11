import { useMemo, useState } from "react"
import {
  ChevronDownIcon,
  ChevronRightIcon,
  ContainerIcon,
  PackageIcon,
  SearchIcon,
} from "lucide-react"

import { EmptyState } from "@/components/empty-state"
import { Badge } from "@/components/ui/badge"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { useProjects, useRegistry } from "@/hooks/use-projects"
import type { ProjectSummary } from "@/lib/types"
import { formatSize } from "@/lib/format"

/** "Available Containers (from GitLab)" — the dashboard sidebar tree
 * (docs/UI-DESIGN.md § 1). Tags become drag sources in the deployment
 * phase. */
export function ContainersPanel() {
  const projects = useProjects()
  const [search, setSearch] = useState("")

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    if (!q) return projects.data ?? []
    return (projects.data ?? []).filter((p) => p.path_with_namespace.toLowerCase().includes(q))
  }, [projects.data, search])

  if (projects.isPending) {
    return (
      <div className="space-y-2">
        <Skeleton className="h-8 w-full" />
        <Skeleton className="h-8 w-full" />
        <Skeleton className="h-8 w-3/4" />
      </div>
    )
  }
  if (projects.isError) {
    return (
      <EmptyState
        icon={ContainerIcon}
        title="Could not load projects"
        description="GitLab may be unreachable. Retry shortly."
        className="border-0 p-6"
      />
    )
  }
  if (projects.data.length === 0) {
    return (
      <EmptyState
        icon={ContainerIcon}
        title="No projects visible"
        description="Your GitLab account has no projects with visibility here, or no instance is onboarded yet."
        className="border-0 p-6"
      />
    )
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="relative">
        <SearchIcon className="absolute top-2.5 left-2.5 size-4 text-muted-foreground" aria-hidden />
        <Input
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search projects…"
          className="h-9 pl-8"
        />
      </div>
      <div className="flex flex-col gap-1">
        {filtered.map((project) => (
          <ProjectNode key={project.id} project={project} />
        ))}
        {filtered.length === 0 ? (
          <p className="p-2 text-center text-sm text-muted-foreground">No matches.</p>
        ) : null}
      </div>
    </div>
  )
}

function ProjectNode({ project }: { project: ProjectSummary }) {
  const [open, setOpen] = useState(false)
  const registry = useRegistry(project.id, open)

  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger className="flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-sm hover:bg-accent">
        {open ? (
          <ChevronDownIcon className="size-3.5 shrink-0" aria-hidden />
        ) : (
          <ChevronRightIcon className="size-3.5 shrink-0" aria-hidden />
        )}
        <span className="truncate text-left">{project.path_with_namespace}</span>
      </CollapsibleTrigger>
      <CollapsibleContent className="ml-3 border-l pl-2">
        {registry.isPending ? (
          <div className="space-y-1 py-1">
            <Skeleton className="h-7 w-full" />
            <Skeleton className="h-7 w-2/3" />
          </div>
        ) : registry.isError ? (
          <p className="px-2 py-1.5 text-xs text-muted-foreground">Registry unavailable.</p>
        ) : registry.data.repositories.length === 0 ? (
          <p className="px-2 py-1.5 text-xs text-muted-foreground">No container images.</p>
        ) : (
          registry.data.repositories.map((repo) => (
            <div key={repo.id} className="py-1">
              <p className="truncate px-2 text-xs font-medium text-muted-foreground">
                {repo.path}
              </p>
              {repo.tags.length === 0 ? (
                <p className="px-2 py-1 text-xs text-muted-foreground">No tags.</p>
              ) : (
                repo.tags.map((tag) => (
                  <div
                    key={tag.name}
                    className="flex items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-accent"
                  >
                    <PackageIcon className="size-4 shrink-0 text-muted-foreground" aria-hidden />
                    <span className="truncate">{repo.path.split("/").pop()}</span>
                    <Badge variant="secondary" className="ml-auto shrink-0 font-mono text-[10px]">
                      {tag.name}
                    </Badge>
                    {tag.size_bytes != null ? (
                      <span className="shrink-0 text-xs text-muted-foreground">
                        {formatSize(tag.size_bytes)}
                      </span>
                    ) : null}
                  </div>
                ))
              )}
            </div>
          ))
        )}
      </CollapsibleContent>
    </Collapsible>
  )
}
