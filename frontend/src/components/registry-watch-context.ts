import { createContext, useContext } from "react"

/** Shared "new image" badge state, produced by RegistryWatchProvider and
 * read by the containers sidebar. Split from the provider component so
 * this file exports no component (keeps react-refresh/HMR happy, like
 * confirm-context). */
export type RegistryWatch = {
  /** Does this project have a repo with an unseen new tag? */
  isProjectNew: (projectId: string) => boolean
  /** Does this repo (by registry path) have an unseen new tag? */
  isRepoNew: (repoPath: string) => boolean
  /** Mark a project's new tags as seen — called when the user collapses
   * it after viewing. */
  markProjectSeen: (projectId: string) => void
}

const NOOP: RegistryWatch = {
  isProjectNew: () => false,
  isRepoNew: () => false,
  markProjectSeen: () => {},
}

export const RegistryWatchContext = createContext<RegistryWatch>(NOOP)

export function useRegistryWatch(): RegistryWatch {
  return useContext(RegistryWatchContext)
}
