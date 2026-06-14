import { useInfiniteQuery } from "@tanstack/react-query"

import { api, queryKeys } from "@/lib/api"
import type { AuditPage } from "@/lib/types"

const PAGE_SIZE = 50

/** Append-only audit trail (GET /api/audit), newest-first and
 * cursor-paginated. `action` (null = all) filters server-side; the
 * controller already scopes non-admins to their own actions. Pages walk
 * backwards via `next_cursor`; a slow poll keeps the head fresh. */
export function useAuditLog(action: string | null) {
  return useInfiniteQuery({
    queryKey: queryKeys.audit(action),
    queryFn: ({ pageParam }) => {
      const params = new URLSearchParams({ limit: String(PAGE_SIZE) })
      if (pageParam) params.set("before", pageParam)
      if (action) params.set("action", action)
      return api<AuditPage>(`/api/audit?${params.toString()}`)
    },
    initialPageParam: null as string | null,
    getNextPageParam: (lastPage) => lastPage.next_cursor,
    refetchInterval: 30_000,
  })
}
