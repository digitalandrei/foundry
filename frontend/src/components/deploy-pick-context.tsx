import { createContext, useContext, type ReactNode } from "react"

import type { DragTagData } from "@/lib/types"

/** Tap-to-deploy plumbing: lets a container tag deep in the sidebar tree
 * open the slot picker without prop-drilling a callback through the
 * project/repo nodes. The dashboard provides it; the tag rows consume it.
 * Same pattern as registry-watch-context. */
const DeployPickContext = createContext<((tag: DragTagData) => void) | null>(null)

export function DeployPickProvider({
  onPick,
  children,
}: {
  onPick: (tag: DragTagData) => void
  children: ReactNode
}) {
  return <DeployPickContext.Provider value={onPick}>{children}</DeployPickContext.Provider>
}

/** Returns "open the slot picker for this tag", or null when rendered
 * outside a provider (i.e. a drag-only context). */
// eslint-disable-next-line react-refresh/only-export-components -- context hook lives with its provider (one small file).
export function useDeployPick(): ((tag: DragTagData) => void) | null {
  return useContext(DeployPickContext)
}
