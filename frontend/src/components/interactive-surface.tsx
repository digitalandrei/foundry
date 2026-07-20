import type { ComponentPropsWithoutRef, Ref } from "react"

import { activateOnEnterOrSpace } from "@/lib/a11y"

type InteractiveSurfaceProps = Omit<
  ComponentPropsWithoutRef<"div">,
  "aria-label" | "onClick" | "onKeyDown" | "role" | "tabIndex"
> & {
  nodeRef?: Ref<HTMLDivElement>
  onActivate?: () => void
  activationLabel?: string
}

/** A dnd-compatible div that acquires complete button keyboard semantics only
 * when it has an action. Free drop-only surfaces remain out of the tab order. */
export function InteractiveSurface({
  nodeRef,
  onActivate,
  activationLabel,
  ...props
}: InteractiveSurfaceProps) {
  return (
    <div
      {...props}
      ref={nodeRef}
      onClick={onActivate}
      role={onActivate ? "button" : undefined}
      tabIndex={onActivate ? 0 : undefined}
      aria-label={onActivate ? activationLabel : undefined}
      onKeyDown={
        onActivate ? (event) => activateOnEnterOrSpace(event, onActivate) : undefined
      }
    />
  )
}
