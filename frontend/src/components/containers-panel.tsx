import { useEffect, useMemo, useRef, useState } from "react"
import { useDraggable } from "@dnd-kit/core"
import {
  ChevronDownIcon,
  ChevronRightIcon,
  ContainerIcon,
  GripVerticalIcon,
  PackageIcon,
  SearchIcon,
} from "lucide-react"

import { EmptyState } from "@/components/empty-state"
import { useRegistryWatch } from "@/components/registry-watch-context"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { useProjects, useRegistry } from "@/hooks/use-projects"
import type { DragTagData, ProjectSummary, RegistryRepository, RegistryTag } from "@/lib/types"
import { formatSize } from "@/lib/format"
import { cn } from "@/lib/utils"

/** Tags shown per repository before "Show all" (registries accumulate
 * dozens of versions — docs/UI-DESIGN.md § sidebar). */
const TAG_PREVIEW_COUNT = 8

function matches(haystack: string, q: string): boolean {
  return haystack.toLowerCase().includes(q)
}

/** "Available Containers (from GitLab)" — the dashboard sidebar tree.
 * The search box filters project paths AND, inside expanded projects,
 * repository paths + tag names (lazy loading means tags of collapsed
 * projects can't be searched until expanded). The list scrolls on its
 * own; the search box stays put. */
export function ContainersPanel() {
  const projects = useProjects()
  const [search, setSearch] = useState("")
  const [openIds, setOpenIds] = useState<ReadonlySet<string>>(new Set())

  const q = search.trim().toLowerCase()

  const pathMatchCount = useMemo(
    () => (projects.data ?? []).filter((p) => !q || matches(p.path_with_namespace, q)).length,
    [projects.data, q],
  )

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

  const setOpen = (id: string, open: boolean) => {
    setOpenIds((prev) => {
      const next = new Set(prev)
      if (open) {
        next.add(id)
      } else {
        next.delete(id)
      }
      return next
    })
  }

  return (
    <div className="flex h-full min-h-0 flex-col gap-2">
      <div className="relative shrink-0">
        <SearchIcon className="absolute top-2.5 left-2.5 size-4 text-muted-foreground" aria-hidden />
        <Input
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search projects, images, tags…"
          className="h-9 pl-8"
        />
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto pr-1">
        <div className="flex flex-col gap-1">
          {projects.data.map((project) => (
            <ProjectNode
              key={project.id}
              project={project}
              query={q}
              open={openIds.has(project.id)}
              onOpenChange={(open) => setOpen(project.id, open)}
            />
          ))}
          {q && pathMatchCount === 0 && openIds.size === 0 ? (
            <p className="p-2 text-center text-sm text-muted-foreground">
              No matches. Tags of collapsed projects aren't searched — expand a project to search
              inside it.
            </p>
          ) : null}
        </div>
      </div>
    </div>
  )
}

function ProjectNode({
  project,
  query,
  open,
  onOpenChange,
}: {
  project: ProjectSummary
  query: string
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const registry = useRegistry(project.id, open)
  const { isProjectNew, markProjectSeen } = useRegistryWatch()
  const projectNew = isProjectNew(project.id)
  // Clear the badge once the user has opened the project and collapsed it
  // again (they've seen what's new inside).
  const wasOpen = useRef(false)
  useEffect(() => {
    if (open) {
      wasOpen.current = true
    } else if (wasOpen.current) {
      wasOpen.current = false
      markProjectSeen(project.id)
    }
  }, [open, project.id, markProjectSeen])

  const pathMatched = !query || matches(project.path_with_namespace, query)

  // Inside an expanded project the query narrows repos/tags; a project
  // whose own path doesn't match stays visible only while it has
  // matching content loaded.
  const filteredRepos = useMemo(() => {
    const repos = registry.data?.repositories ?? []
    if (!query) return repos
    return repos
      .map((repo) => {
        if (matches(repo.path, query)) return repo
        const tags = repo.tags.filter((t) => matches(t.name, query))
        return tags.length > 0 ? { ...repo, tags } : null
      })
      .filter((r): r is RegistryRepository => r !== null)
  }, [registry.data, query])

  const contentMatches = open && query !== "" && filteredRepos.length > 0
  if (!pathMatched && !contentMatches) {
    return null
  }

  return (
    <Collapsible open={open} onOpenChange={onOpenChange}>
      <CollapsibleTrigger className="flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-sm hover:bg-accent">
        {open ? (
          <ChevronDownIcon className="size-3.5 shrink-0" aria-hidden />
        ) : (
          <ChevronRightIcon className="size-3.5 shrink-0" aria-hidden />
        )}
        <span className="min-w-0 truncate text-left">{project.path_with_namespace}</span>
        {!open && projectNew ? (
          <span
            className="ml-auto size-2 shrink-0 rounded-full bg-primary"
            aria-label="new images available"
          />
        ) : null}
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
        ) : filteredRepos.length === 0 ? (
          <p className="px-2 py-1.5 text-xs text-muted-foreground">No images match the search.</p>
        ) : (
          filteredRepos.map((repo) => <RepoNode key={repo.id} repo={repo} filtered={!!query} />)
        )}
      </CollapsibleContent>
    </Collapsible>
  )
}

/** Drag source: drop on a free slot to deploy, on a running slot to
 * replace (docs/UI-DESIGN.md § Drag interaction). */
function DraggableTag({ tag, imageName }: { tag: RegistryTag; imageName: string }) {
  const data: DragTagData = {
    registryTagId: tag.id,
    imageName,
    tagName: tag.name,
    sizeBytes: tag.size_bytes,
  }
  const { attributes, listeners, setNodeRef, isDragging } = useDraggable({
    id: `tag-${tag.id}`,
    data,
  })

  return (
    <div
      ref={setNodeRef}
      {...attributes}
      {...listeners}
      className={cn(
        "flex cursor-grab items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-accent",
        isDragging && "opacity-40",
      )}
      aria-label={`Drag ${imageName}:${tag.name} to a GPU slot to deploy`}
    >
      <GripVerticalIcon className="size-3 shrink-0 text-muted-foreground/60" aria-hidden />
      <PackageIcon className="size-4 shrink-0 text-muted-foreground" aria-hidden />
      <span className="truncate">{imageName}</span>
      <Badge variant="secondary" className="ml-auto shrink-0 font-mono text-[10px]">
        {tag.name}
      </Badge>
      {tag.size_bytes != null ? (
        <span className="shrink-0 text-xs text-muted-foreground">
          {formatSize(tag.size_bytes)}
        </span>
      ) : null}
    </div>
  )
}

function RepoNode({ repo, filtered }: { repo: RegistryRepository; filtered: boolean }) {
  const [showAll, setShowAll] = useState(false)
  const repoNew = useRegistryWatch().isRepoNew(repo.path)
  // A filtered view is already narrowed — show everything that matched.
  const visible = filtered || showAll ? repo.tags : repo.tags.slice(0, TAG_PREVIEW_COUNT)
  const hidden = repo.tags.length - visible.length
  const imageName = repo.path.split("/").pop()

  return (
    <div className="py-1">
      <div className="flex items-center gap-1.5 px-2">
        <p className="truncate text-xs font-medium text-muted-foreground">{repo.path}</p>
        {repoNew ? (
          <Badge className="shrink-0 px-1.5 py-0 text-[9px] leading-4">new</Badge>
        ) : null}
      </div>
      {repo.tags.length === 0 ? (
        <p className="px-2 py-1 text-xs text-muted-foreground">No tags.</p>
      ) : (
        <>
          {visible.map((tag) => (
            <DraggableTag key={tag.name} tag={tag} imageName={imageName ?? "image"} />
          ))}
          {hidden > 0 ? (
            <Button
              variant="ghost"
              size="sm"
              className="h-7 w-full justify-start px-2 text-xs text-muted-foreground"
              onClick={() => setShowAll(true)}
            >
              Show {hidden} more tag{hidden === 1 ? "" : "s"}…
            </Button>
          ) : null}
          {showAll && !filtered && repo.tags.length > TAG_PREVIEW_COUNT ? (
            <Button
              variant="ghost"
              size="sm"
              className="h-7 w-full justify-start px-2 text-xs text-muted-foreground"
              onClick={() => setShowAll(false)}
            >
              Show fewer
            </Button>
          ) : null}
        </>
      )}
    </div>
  )
}
