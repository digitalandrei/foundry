# Phase 8 — UI (Full Dashboard, Dark + Light)

**Status:** 🔶 In progress — functional UI shipped incrementally across
earlier phases (10 pages, slot grid, deployment-detail page, fleet Telemetry
tab, query-backed Audit table). Remaining: the formal polish pass below —
light-mode-complete sweep, accessibility/keyboard-dnd, and
empty/loading/error states everywhere.

## Goal

Complete the frontend to the approved design (`../UI-DESIGN.md`): all pages,
polish, both themes. Earlier phases shipped functional UI incrementally;
this phase closes the gaps.

> Decided early (Phase 3): the serving model is **Nginx static SPA +
> API-only controller** — that decision point is closed
> (`../DEPLOYMENT.md`).

## Deliverables

- Dashboard finalized per mockup: sidebar (filters, favorites, "show only
  mine", system status), slot grid (legend, drag ghost, drop-target
  highlight, offline rendering), running-deployments panel
- Pages: Deployments (filters + detail with lifecycle timeline), Servers
  (enrollment + detail), Audit Logs (filterable; ✅ shipped 0.25.0 —
  `GET /api/audit` + cursor-paginated, action-filtered table), Settings
  (instances, tokens, theme, profile)
- **Light mode complete**: all semantic tokens defined for `:root` and
  `.dark`; theme switcher persisted; both themes pass the checklist on
  every screen
- Live updates strategy implemented (SSE or polling per Phase 7 decision)
- Accessibility pass: keyboard drag-and-drop path, dialog semantics, focus
  management
- Empty/loading/error states everywhere (`../FRONTEND_RULES.md` checklist)

## Acceptance

- All success-criteria flows (`../ROADMAP.md`) achievable from the UI in
  both themes; no hardcoded palette colors (`grep` gate for raw hex/palette
  classes in components)
