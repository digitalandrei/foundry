import { useCallback, useRef, useState } from "react"

import { ConfirmContext, type ConfirmFn, type ConfirmOptions } from "@/components/confirm-context"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

/** Imperative confirmation modal (replaces native `window.confirm` —
 * docs/FRONTEND_RULES.md / UI-DESIGN). `useConfirm()` (see
 * confirm-context) opens the modal and resolves true/false. Destructive
 * actions (Stop, Delete) get a red CONFIRM button. */
export function ConfirmProvider({ children }: { children: React.ReactNode }) {
  const [opts, setOpts] = useState<ConfirmOptions | null>(null)
  const [typed, setTyped] = useState("")
  const resolver = useRef<((v: boolean) => void) | null>(null)

  const confirm = useCallback<ConfirmFn>((o) => {
    setTyped("")
    setOpts(o)
    return new Promise<boolean>((resolve) => {
      resolver.current = resolve
    })
  }, [])

  const settle = (v: boolean) => {
    resolver.current?.(v)
    resolver.current = null
    setOpts(null)
    setTyped("")
  }

  // A type-to-confirm prompt gates the button until the text matches.
  const needText = opts?.requireConfirmText
  const confirmEnabled = !needText || typed === needText

  return (
    <ConfirmContext.Provider value={confirm}>
      {children}
      <Dialog open={opts !== null} onOpenChange={(open) => !open && settle(false)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{opts?.title}</DialogTitle>
            {opts?.description ? (
              <DialogDescription>{opts.description}</DialogDescription>
            ) : null}
          </DialogHeader>
          {needText ? (
            <div className="space-y-2">
              <Label htmlFor="confirm-text">
                Type <span className="font-mono font-medium">{needText}</span> to confirm
              </Label>
              <Input
                id="confirm-text"
                value={typed}
                onChange={(e) => setTyped(e.target.value)}
                autoComplete="off"
                autoFocus
              />
            </div>
          ) : null}
          <DialogFooter>
            <Button variant="outline" onClick={() => settle(false)}>
              Cancel
            </Button>
            <Button
              variant={opts?.destructive ? "destructive" : "default"}
              disabled={!confirmEnabled}
              onClick={() => settle(true)}
            >
              {opts?.confirmLabel ?? "CONFIRM"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </ConfirmContext.Provider>
  )
}
