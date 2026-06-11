# Phase 1 — Repository Bootstrap

**Status:** Not started · refine this plan right before starting.

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
