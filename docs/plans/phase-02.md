# Phase 2 — Workspace Creation

**Status:** ✅ Done (2026-06-11).

Delivered: `shared` wire contract (7 state enums with canonical strings
+ tests, 13 UUIDv7 ID newtypes, error/health DTOs); initial 19-table
migration applied to the live `foundry` DB; controller skeleton (env
config, `/health` with DB check on `127.0.0.1:8400`, embedded
migrations, JSON tracing, graceful shutdown — verified live with the
agent polling it); agent skeleton (TOML config, rustls client,
connectivity loop, SIGTERM-clean); frontend shell (Vite + React 19 + TS
strict + Tailwind 4 + shadcn nova preset, dark default + light via
`next-themes`, slot-state tokens + `lib/states.ts` map, TanStack
Query/Router, 5 pages with real empty states, version in sidebar);
systemd + nginx drafts in `deployment/`. `scripts/check.sh` covers
fmt/clippy/test/frontend-build and passes.

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
