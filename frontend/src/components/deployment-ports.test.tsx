import "@testing-library/jest-dom/vitest"
import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it } from "vitest"

import {
  DeploymentPorts,
  DeploymentPrimaryUrl,
} from "@/components/deployment-ports"
import type { DeploymentPort } from "@/lib/types"

afterEach(cleanup)

const mapped: DeploymentPort = {
  container_port: 8188,
  host_port: 20000,
  protocol: "tcp",
  kind: "HTTP",
  hostname: "comfy-auto.protv-ai-04-03.ai.protv.ro",
}

describe.each(["light", "dark"])("deployment app address (%s theme)", (theme) => {
  it("renders the real HTTPS URL as a link in compact and full views", () => {
    render(
      <div className={theme}>
        <DeploymentPrimaryUrl ports={[mapped]} />
        <DeploymentPorts ports={[mapped]} />
      </div>,
    )
    const links = screen.getAllByRole("link", {
      name: /https:\/\/comfy-auto\.protv-ai-04-03\.ai\.protv\.ro/i,
    })
    expect(links).toHaveLength(2)
    for (const link of links) {
      expect(link).toHaveAttribute(
        "href",
        "https://comfy-auto.protv-ai-04-03.ai.protv.ro",
      )
    }
  })
})
