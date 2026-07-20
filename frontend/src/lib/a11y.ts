import type { KeyboardEvent } from "react"

/** Keyboard equivalent for a non-native interactive surface that must remain
 * a div because dnd-kit owns the same drop target. */
export function activateOnEnterOrSpace(
  event: KeyboardEvent,
  activate: () => void,
) {
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault()
    activate()
  }
}
