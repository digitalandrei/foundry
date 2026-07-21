import { afterEach, describe, expect, it, vi } from "vitest"

import { api } from "@/lib/api"

afterEach(() => {
  vi.unstubAllGlobals()
})

describe("api", () => {
  it.each([202, 204])("accepts a successful empty %s response", async (status) => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(null, { status })))

    await expect(api<void>("/api/command", { method: "POST" })).resolves.toBeUndefined()
  })

  it("parses a successful JSON response", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ status: "ok" }), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        }),
      ),
    )

    await expect(api<{ status: string }>("/health")).resolves.toEqual({ status: "ok" })
  })
})
