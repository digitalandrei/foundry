# Frontend Rules for Foundry

Quality bar for `frontend/` (React + TypeScript strict + Vite + shadcn/ui +
TanStack Query + TanStack Router + dnd-kit). Visual/UX specifics live in
`UI-DESIGN.md`; this file is about code.

## Structure & Reuse

```
frontend/src/
  components/ui/   # shadcn primitives — never modified directly
  components/      # app components composed from primitives (shared!)
  pages/           # route-level components, thin — composition only
  hooks/           # query/mutation hooks, drag-and-drop hooks
  lib/             # API client, types mirrored from shared/, utils (cn)
```

- **No god files.** Pages compose; logic lives in hooks; presentation lives
  in small components. A component used twice moves to `components/`.
- **Reuse styles**: one slot chip component, one state-color map, one status
  badge — used everywhere a slot/deployment state appears. Never re-derive
  state colors locally (single source: the state→color map in `lib/`,
  matching `UI-DESIGN.md`).
- Types mirror the `shared` crate DTOs — one types module, kept in sync with
  the Rust contract (drift here is a bug).

## Server State

- TanStack Query is the source of truth for all server data; no raw `fetch`
  in components, no copying query data into `useState`.
- Every mutation invalidates the affected query keys (deployments, servers,
  slots) — the dashboard must reflect a deploy/stop/replace without reload.
- Centralized query-key factory in `lib/`; no inline string keys.
- Polling/refresh strategy for live slot states is defined in Phase 8 (SSE
  preferred over polling when added — record the decision here).

## Routing & Forms

- TanStack Router; URL is state for filters/selection (server, project,
  search), so views are shareable.
- Forms: react-hook-form + zod + shadcn form primitives — the **Field
  family** (`Field`, `FieldLabel`, `FieldError`, …; shadcn 4.x replaced
  the old `Form` wrapper). Zod schemas validate deployment configs
  (ports, env, volumes) client-side; the server revalidates. Reference
  implementation: `components/instance-admin.tsx`.

## shadcn/ui & Styling

- Use existing `components/ui/` primitives first; compose, don't reinvent;
  add missing primitives via `npx shadcn@latest add <component>`.
- Variants via `cva`; conditional classes via `cn()`. No inline styles, no
  CSS modules.
- **Semantic colors only** (CSS variables) — required for dark/light theming
  (see `UI-DESIGN.md` § Theming). Raw hex/Tailwind palette colors in
  components are a review blocker; state colors come from the shared map.
- Icons: lucide-react, specific imports, `h-4 w-4` inline / `h-5 w-5` in
  buttons.

## Drag & Drop (dnd-kit)

- One `DndContext` at the dashboard level; draggable = registry tag card,
  droppable = slot chip.
- Drop on a `FREE` slot → deployment config dialog. Drop on an occupied
  slot → replacement confirmation dialog (`ARCHITECTURE.md` § Replacement
  workflow). Invalid targets (OFFLINE/FAILED/incompatible) don't activate.
- Keyboard accessibility: dnd-kit keyboard sensor enabled; drag-and-drop is
  an enhancement — every action also has a button path.

## Quality Checklist

- TypeScript strict, no `any`.
- Loading, error, and empty states for every query-backed view.
- Both themes verified for every new screen (dark default + light).
- Accessible: dialog titles, ARIA on drag handles, keyboard nav.
- `npm run build` clean (this is the verification gate for frontend work).
