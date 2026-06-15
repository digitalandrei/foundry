import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { RegistryWatchContext, type RegistryWatch } from "@/components/registry-watch-context"
import { useRegistryUpdates } from "@/hooks/use-projects"
import { queryKeys } from "@/lib/api"

/** Polls GET /api/registry/updates while the user is in the app, toasts
 * newly-pushed image tags across their available repos, badges the
 * affected projects/repos in the sidebar until viewed, and refreshes any
 * expanded project tree so the new tag shows up. The first response of a
 * session is a silent baseline — it primes the mirror without toasting
 * the backlog (docs/UI-DESIGN.md § sidebar). */
export function RegistryWatchProvider({ children }: { children: React.ReactNode }) {
  const qc = useQueryClient()
  const updates = useRegistryUpdates()
  const baselined = useRef(false)
  // repoPath -> projectId, for repos whose new tag hasn't been viewed.
  const [unseen, setUnseen] = useState<ReadonlyMap<string, string>>(new Map())

  useEffect(() => {
    const data = updates.data
    if (!data) return
    if (!baselined.current) {
      // First response of the session: prime the mirror silently.
      baselined.current = true
      return
    }
    if (data.new_tags.length === 0) return

    if (data.new_tags.length === 1) {
      const t = data.new_tags[0]
      toast.info(`New image: ${t.repo_path}:${t.tag_name}`)
    } else {
      const head = data.new_tags
        .slice(0, 4)
        .map((t) => `${t.repo_path.split("/").pop()}:${t.tag_name}`)
        .join(", ")
      toast.info(`${data.new_tags.length} new container images`, {
        description: data.new_tags.length > 4 ? `${head} …` : head,
      })
    }

    // Accumulate badge state from the poll feed (an external system).
    // Fires only on a genuine new-tag event — rare, so no cascading-render
    // concern; this is an external-sync, not a render derivation.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setUnseen((prev) => {
      const next = new Map(prev)
      for (const t of data.new_tags) next.set(t.repo_path, t.project_id)
      return next
    })

    // Refresh any *expanded* project tree so the tag appears; a collapsed
    // (un-mounted/disabled) registry query just ignores the invalidation.
    for (const projectId of new Set(data.new_tags.map((t) => t.project_id))) {
      qc.invalidateQueries({ queryKey: queryKeys.registry(projectId) })
    }
  }, [updates.data, qc])

  const isProjectNew = useCallback(
    (projectId: string) => {
      for (const pid of unseen.values()) if (pid === projectId) return true
      return false
    },
    [unseen],
  )
  const isRepoNew = useCallback((repoPath: string) => unseen.has(repoPath), [unseen])
  const markProjectSeen = useCallback((projectId: string) => {
    setUnseen((prev) => {
      let changed = false
      const next = new Map(prev)
      for (const [path, pid] of next) {
        if (pid === projectId) {
          next.delete(path)
          changed = true
        }
      }
      return changed ? next : prev
    })
  }, [])

  const value = useMemo<RegistryWatch>(
    () => ({ isProjectNew, isRepoNew, markProjectSeen }),
    [isProjectNew, isRepoNew, markProjectSeen],
  )

  return <RegistryWatchContext.Provider value={value}>{children}</RegistryWatchContext.Provider>
}
