# Codebase Map

File routing by feature. **Status: PLANNED** — the workspace is created in
Phases 1–2. Until then this map describes the intended layout from
`../ARCHITECTURE.md` § Workspace Layout; replace "planned" notes with real
paths as code lands (same commit set).

## Top Level

| Path | Contents | Status |
|---|---|---|
| `controller/` | `foundry-controller` binary — axum API, OAuth, scheduler, task queue, GitLab clients | planned |
| `agent/` | `foundry-agent` binary — task loop, Docker (bollard), NVML inventory | planned |
| `shared/` | Wire contract: DTOs, state enums, ID newtypes, validation | planned |
| `frontend/` | React + TS + Vite + shadcn SPA | planned |
| `migrations/` | sqlx MySQL migrations | planned |
| `deployment/` | systemd units, nginx vhost, agent install script | planned |
| `scripts/` | check.sh and dev helpers | planned |
| `docs/` | knowledge base (you are here) | live |
| `.claude/` | agents, skills, hooks, settings | live |

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
