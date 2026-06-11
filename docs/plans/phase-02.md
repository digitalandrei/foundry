# Phase 2 — Workspace Creation

**Status:** Not started · refine this plan right before starting.

## Goal

Real crate/app scaffolding: compiling controller and agent binaries, the
shared contract crate, the Vite frontend, and the initial database schema.

## Deliverables

- `shared/`: state enums (slot states, deployment lifecycle, task types per
  `../ARCHITECTURE.md`), ID newtypes, DTO modules, validation helpers
- `controller/`: axum app skeleton (router, `/health`, config from env,
  sqlx MySQL pool, tracing JSON setup, error envelope per `../API.md`)
- `agent/`: binary skeleton (config load from `/etc/foundry-agent/config.toml`,
  HTTPS client, main loop scaffold, tracing)
- `migrations/`: initial migration set creating all 19 tables
  (`../DATABASE.md`)
- `frontend/`: Vite + React + TS strict + Tailwind + shadcn/ui init, theme
  tokens (dark default + light, `../UI-DESIGN.md` § Theming), TanStack
  Query/Router setup, layout shell (top nav + sidebar skeleton)
- `deployment/`: draft systemd units + nginx vhost template
  (`../DEPLOYMENT.md`)

## Acceptance

- `cargo test` green; `sqlx migrate run` applies cleanly to a fresh DB
- `npm run build` green; app shell renders in both themes
- `docs/ai/codebase-map.md` updated with real paths
