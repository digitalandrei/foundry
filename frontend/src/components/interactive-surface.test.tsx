import "@testing-library/jest-dom/vitest"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { InteractiveSurface } from "@/components/interactive-surface"

afterEach(cleanup)

describe.each(["light", "dark"])("interactive GPU surface (%s theme)", (theme) => {
  it("is named, focusable, and activates with click, Enter, and Space", () => {
    const activate = vi.fn()
    render(
      <div className={theme}>
        <InteractiveSurface activationLabel="Open deployment trainer" onActivate={activate}>
          trainer
        </InteractiveSurface>
      </div>,
    )
    const surface = screen.getByRole("button", { name: "Open deployment trainer" })
    surface.focus()
    expect(surface).toHaveFocus()
    fireEvent.click(surface)
    fireEvent.keyDown(surface, { key: "Enter" })
    fireEvent.keyDown(surface, { key: " " })
    expect(activate).toHaveBeenCalledTimes(3)
  })

  it("keeps a drop-only surface out of the tab order", () => {
    const { container } = render(<InteractiveSurface data-testid="drop-only" />)
    expect(screen.queryByRole("button")).not.toBeInTheDocument()
    expect(container.querySelector('[data-testid="drop-only"]')).not.toHaveAttribute("tabindex")
  })
})
