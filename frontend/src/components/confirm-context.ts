import { createContext, useContext } from "react"
import type { ReactNode } from "react"

/** Options for an imperative confirmation (see ConfirmProvider in
 * confirm-dialog.tsx). Split out from the component so the provider file
 * exports only a component — keeps react-refresh/HMR happy. */
export type ConfirmOptions = {
  title: string
  description?: ReactNode
  /** Defaults to "CONFIRM". */
  confirmLabel?: string
  destructive?: boolean
}

export type ConfirmFn = (opts: ConfirmOptions) => Promise<boolean>

export const ConfirmContext = createContext<ConfirmFn | null>(null)

/** Returns the imperative confirm() from the nearest ConfirmProvider. */
export function useConfirm() {
  const ctx = useContext(ConfirmContext)
  if (!ctx) throw new Error("useConfirm must be used within ConfirmProvider")
  return ctx
}
