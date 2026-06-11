# Codebase Map

File routing by feature. The workspace skeleton exists (Phase 1); crate
internals and the frontend scaffold land in Phase 2 — update the
feature table below from "planned" to real paths as they land (same
commit set).

## Top Level

| Path | Contents | Status |
|---|---|---|
| `controller/` | `foundry-controller` binary — axum API, OAuth, scheduler, task queue, GitLab clients | skeleton (compiles, no logic) |
| `agent/` | `foundry-agent` binary — task loop, Docker (bollard), NVML inventory | skeleton (compiles, no logic) |
| `shared/` | Wire contract: DTOs, state enums, ID newtypes, validation | skeleton (empty lib) |
| `frontend/` | React + TS + Vite + shadcn SPA | placeholder README (scaffold in Phase 2) |
| `migrations/` | sqlx MySQL migrations | empty (initial schema in Phase 2) |
| `deployment/` | systemd units, nginx vhost, agent install script | README only (drafts in Phase 2) |
| `scripts/` | `check.sh` — fmt + clippy `-D warnings` + test, the standard gate | live |
| `docs/` | knowledge base (you are here) | live |
| `.claude/` | agents, skills, hooks, settings | live |

Dev environment: `/opt/foundry/.env` (gitignored, mode 600) holds
`DATABASE_URL` for the local `foundry` database.

## Feature → Location (planned)

- Slot/deployment/task **state enums** → `shared/src/states.rs` (single
  source of truth; DB columns and frontend types mirror it)
- API DTOs → `shared/src/dto/`
- Controller routes → `controller/src/routes/` (grouped by resource:
  `auth`, `me`, `instances`, `projects`, `registry`, `servers`,
  `deployments`, `audit`, `agent`)
- State-machine transition functions → `controller/src/lifecycle/`
- GitLab clients (OAuth, API, registry token) → `controller/src/gitlab/`
- Task queue dispatch → `controller/src/tasks/`
- Agent task executors → `agent/src/tasks/`
- NVML/inventory → `agent/src/inventory/`
- Frontend pages → `frontend/src/pages/` (dashboard, deployments, servers,
  audit, settings)
- Slot chip / state-color map → `frontend/src/components/` +
  `frontend/src/lib/states.ts`
- Theme tokens → `frontend/src/index.css` (`:root` + `.dark`)

## Maintenance

When modules move or appear, update this file in the same commit set. The
doc-drift hook nudges on code changes without a matching docs change.
