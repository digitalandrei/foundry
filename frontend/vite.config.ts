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
})
