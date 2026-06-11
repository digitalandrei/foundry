# Foundry Frontend

React 19 + TypeScript (strict) + Vite + Tailwind 4 + shadcn/ui +
TanStack Query/Router. Design contract: `../docs/UI-DESIGN.md`; code
rules: `../docs/FRONTEND_RULES.md`.

```bash
npm install
npm run dev       # dev server
npm run build     # the verification gate (tsc -b && vite build)
npm run lint
```

Conventions wired in:

- **Theming**: dark default, light required; semantic tokens only.
  Slot-state tokens (`--slot-*`) live in `src/index.css`; the single
  state→color map is `src/lib/states.ts`.
- **Path alias**: `@/` → `src/`.
- **Version**: `package.json` version → `__APP_VERSION__` →
  `src/lib/version.ts` → dashboard sidebar. Keep in sync with the Cargo
  workspace version.
- shadcn primitives in `src/components/ui/` are generated — compose
  around them, never edit them.
