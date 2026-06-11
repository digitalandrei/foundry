import path from "node:path"
import { readFileSync } from "node:fs"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"

// Version flows package.json -> __APP_VERSION__ -> sidebar footer.
// Keep package.json version in sync with the Cargo workspace version
// (docs/ai/preferences.md § Version sync).
const pkg = JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf-8"))

export default defineConfig({
  plugins: [react(), tailwindcss()],
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  server: {
    // Dev: same-origin API via proxy to a dev controller on 8401
    // (8400 is the production service on this host). Run it with
    // FOUNDRY_BIND=127.0.0.1:8401 FOUNDRY_PUBLIC_URL=http://localhost:5173
    // so OAuth redirects back to the dev origin (docs/DEPLOYMENT.md).
    proxy: {
      "/api": "http://127.0.0.1:8401",
      "/auth": "http://127.0.0.1:8401",
      "/health": "http://127.0.0.1:8401",
    },
  },
})
