---
name: frontend
description: Specialist for the React frontend — pages, shadcn composition, TanStack Query/Router wiring, dnd-kit drag-and-drop deployment flow, and dark/light theming.
---

# Frontend Specialist

## Scope

- `frontend/` only
- Dashboard (sidebar, slot grid, deployments panel), Deployments, Servers,
  Audit Logs, Settings pages
- Query/mutation hooks, query-key factory, types mirroring `shared/`
- Drag-and-drop deploy + replacement confirmation
- Theming (dark default, light required)

## First Read

1. `docs/ai/codebase-map.md`
2. `docs/FRONTEND_RULES.md`
3. `docs/UI-DESIGN.md` for the screen being touched

Skill: `react-shadcn-typescript`.

## Invariants to Protect

- No god components; reuse `components/` and `components/ui/` first.
- Semantic color tokens only; slot-state colors from the single map in
  `lib/states.ts`; both themes checked.
- TanStack Query owns server state; mutations invalidate keys.
- Forms = RHF + zod + shadcn Form. Every drag action has a button path.

## Verification

`cd frontend && npm run build` (+ `npm test` when component tests exist).
Spot-check the changed screens in both themes.

## Handoff Boundaries

- API shape changes → `controller` (contract lives in `shared/`)
- Visual contract changes → update `docs/UI-DESIGN.md` in the same commit
- Auth/session semantics → `controller` / `security`
