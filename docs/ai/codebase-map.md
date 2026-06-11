# Codebase Map

File routing by feature. The workspace skeleton exists (Phase 1); crate
internals and the frontend scaffold land in Phase 2 — update the
feature table below from "planned" to real paths as they land (same
commit set).

## Top Level

| Path | Contents | Status |
|---|---|---|
| `controller/` | `foundry-controller` binary — axum API, OAuth, scheduler, task queue, GitLab clients | live: config, /health, pool, embedded migrations |
| `agent/` | `foundry-agent` binary — task loop, Docker (bollard), NVML inventory | live: config, HTTPS client, connectivity loop |
| `shared/` | Wire contract: DTOs, state enums, ID newtypes | live |
| `frontend/` | React + TS + Vite + shadcn SPA | live: shell, theming, 5 pages |
| `migrations/` | sqlx MySQL migrations (embedded into controller, run at startup) | live: initial 19-table schema |
| `deployment/` | systemd units, nginx vhost (drafts until Phase 10) | drafted |
| `scripts/` | `check.sh` — fmt + clippy `-D warnings` + test + frontend build | live |
| `docs/` | knowledge base (you are here) | live |
| `.claude/` | agents, skills, hooks, settings | live |

Dev environment: `/opt/foundry/.env` (gitignored, mode 600) holds
`DATABASE_URL` for the local `foundry` database.

## Feature → Location

**Live now:**

- State enums (slot/deployment/task/server/actor) →
  `shared/src/states.rs` — single source of truth; DB columns store
  these exact strings; mirrored by `frontend/src/lib/states.ts`
- ID newtypes (UUIDv7) → `shared/src/ids.rs`
- Shared DTOs (error envelope, health) → `shared/src/dto/`
- Controller config / app state / routes → `controller/src/config.rs`,
  `state.rs`, `routes/` (one module per resource; `routes/health.rs`)
- Embedded migrations → `controller/src/main.rs` (`MIGRATOR`) reading
  `migrations/*.sql`
- Agent config (TOML, `FOUNDRY_AGENT_CONFIG` override) →
  `agent/src/config.rs`; poll loop → `agent/src/main.rs`
- Frontend pages → `frontend/src/pages/{dashboard,deployments,servers,audit,settings}.tsx`
- Layout shell / nav → `frontend/src/components/layout/app-shell.tsx`
- Shared UI building blocks → `frontend/src/components/`
  (`empty-state.tsx`, `slot-legend.tsx`, `mode-toggle.tsx`); shadcn
  primitives in `frontend/src/components/ui/` (generated, don't edit)
- State→color map → `frontend/src/lib/states.ts`; theme + slot tokens →
  `frontend/src/index.css` (`:root` + `.dark`); version →
  `frontend/src/lib/version.ts`
- Theming: `next-themes` (`ThemeProvider` in `frontend/src/main.tsx`,
  storage key `foundry-theme`, dark default)

**Planned (later phases):**

- Resource routes `auth`, `me`, `instances`, `projects`, `registry`,
  `servers`, `deployments`, `audit`, `agent` → `controller/src/routes/`
- State-machine transition functions → `controller/src/lifecycle/`
- GitLab clients → `controller/src/gitlab/`
- Task queue dispatch → `controller/src/tasks/`
- Agent task executors → `agent/src/tasks/`; NVML inventory →
  `agent/src/inventory/`
- Frontend query/mutation hooks → `frontend/src/hooks/`

## Maintenance

When modules move or appear, update this file in the same commit set. The
doc-drift hook nudges on code changes without a matching docs change.
