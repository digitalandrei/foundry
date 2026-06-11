---
name: react-shadcn-typescript
description: >
  For the Foundry project at /opt/foundry. Frontend patterns: React +
  TypeScript strict + Vite, shadcn/ui composition, TanStack Query/Router,
  react-hook-form + zod, dnd-kit, dark/light theming with semantic tokens.
  Use when building or reviewing any frontend component, page, form, or
  the drag-and-drop deployment flow.
---

# React + shadcn + TypeScript

Working patterns for `frontend/`. Full rule set: `docs/FRONTEND_RULES.md`;
visual contract: `docs/UI-DESIGN.md`.

## Composition Rules

1. **Use existing components first** — check `components/ui/` (shadcn
   primitives) and `components/` (app components) before writing anything.
2. **Compose, don't reinvent**; never modify `components/ui/` directly.
3. A component used twice moves to `components/`. Pages stay thin.
4. Missing primitive: `npx shadcn@latest add <component>`.

## Styling & Theming

- Tailwind utilities + `cn()` for conditional merging; no inline styles.
- **Semantic tokens only** — `bg-background`, `text-muted-foreground`,
  `border-border`, and the Foundry slot-state tokens (`--slot-free`,
  `--slot-running`, `--slot-reserved`, `--slot-offline`, `--slot-failed`).
  Raw hex or palette classes (`bg-zinc-900`) in components break light
  mode and are a review blocker.
- Dark is the default theme; both themes must be checked for every screen.
- Slot/deployment state colors come from the single map in
  `frontend/src/lib/states.ts` — never re-derived locally.

## Server State (TanStack Query)

- All API access through query/mutation hooks in `hooks/`; no raw `fetch`
  in components; no copying query data into `useState`.
- Query-key factory in `lib/`; every mutation invalidates affected keys
  (deploy → deployments + servers/slots).
- Loading, error, and empty states for every query-backed view.

## Forms

Always react-hook-form + zod + shadcn `Form` components. Zod schemas for
deployment config (ports/env/volumes) live with the form and mirror
server-side validation.

## Drag & Drop (dnd-kit)

- One `DndContext` on the dashboard; container cards are draggables, slot
  chips are droppables; `FREE` slots highlight, occupied slots route to the
  replacement confirmation, OFFLINE/FAILED don't activate.
- Keyboard sensor enabled; every drag action has a button-path equivalent.

## Icons & Misc

- lucide-react, specific imports; `h-4 w-4` inline, `h-5 w-5` buttons.
- Dialogs always have `DialogTitle`. Toasts via sonner for mutation
  feedback.
- TypeScript strict, no `any`. Types mirror `shared/` DTOs in one module.

## Verification

`cd frontend && npm run build` minimum; component tests for state maps,
slot chips, and form schemas (`docs/TESTING.md`).
