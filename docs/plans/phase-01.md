# Phase 1 — Repository Bootstrap

**Status:** ✅ Done (2026-06-11).

Delivered: workspace skeleton (`controller`/`agent`/`shared` compile;
binaries print name+version), `scripts/check.sh` gate (passing),
placeholder READMEs for `frontend/`/`migrations/`/`deployment/`, and —
pulled forward from later phases — the local database: MariaDB DB
`foundry` + `foundry@localhost` user scoped to `foundry.*` only, creds in
gitignored `/opt/foundry/.env`. CI decision recorded in `../ROADMAP.md`
(no hosted CI yet; check.sh is the gate).

## Goal

Turn the repo into a buildable skeleton matching the planned layout in
`../ARCHITECTURE.md` § Workspace Layout.

## Deliverables

- Top-level directories: `controller/`, `agent/`, `shared/`, `frontend/`,
  `migrations/`, `deployment/`, `scripts/`
- Root `Cargo.toml` workspace (members: controller, agent, shared) with
  workspace lints (`unsafe_op_in_unsafe_fn`, `dbg_macro`, `todo` → warn) and
  release profile (strip, lto)
- `.gitignore` already exists from Phase 0; extend if needed
- `scripts/check.sh` — fmt + clippy `-D warnings` + test, the standard gate
- CI decision recorded (GitLab CI vs none initially) in `../ROADMAP.md`
  amendments

## Acceptance

- `cargo build` succeeds on empty crates
- `docs/ai/codebase-map.md` updated from "planned" to actual paths
