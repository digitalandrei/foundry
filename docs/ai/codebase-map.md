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
- Shared DTOs (error, health, instance, me, project, registry) →
  `shared/src/dto/`; frontend mirror → `frontend/src/lib/types.ts`
- Controller config / app state → `controller/src/config.rs`, `state.rs`
- Secrets-at-rest + token hashing → `controller/src/crypto.rs`
  (AES-256-GCM SecretBox, `random_token`, `token_hash`)
- Error envelope → `controller/src/error.rs` (AppError)
- Audit writes → `controller/src/audit.rs` (append-only)
- Sessions + extractors (`CurrentUser`, `AdminUser`), cookies, OAuth
  routes → `controller/src/auth/{session,cookies,routes}.rs`
- GitLab: OAuth/PKCE → `controller/src/gitlab/oauth.rs`; API client
  (pagination caps) → `gitlab/client.rs`; token refresh →
  `gitlab/tokens.rs`; response types → `gitlab/types.rs`
- Data access → `controller/src/repos/{instances,users,mirror,local_admins,servers}.rs`
  (local_admins: argon2id operator accounts; servers: enrollment
  tokens, agent identity, heartbeat, offline sweeper)
- Agent auth extractor → `controller/src/auth/agent.rs`; agent routes →
  `controller/src/routes/{servers,agent}.rs`
- Routes (one module per resource) → `controller/src/routes/{health,me,instances,projects,registry}.rs`
- Bootstrap CLI (`instance add`) → `controller/src/cli.rs`
- Embedded migrations → `controller/src/main.rs` (`MIGRATOR`) reading
  `migrations/*.sql`
- Agent config (TOML, `FOUNDRY_AGENT_CONFIG` override) →
  `agent/src/config.rs`; heartbeat + inventory loops, CLI dispatch →
  `agent/src/main.rs`; `--register` (enroll, self-install, user, unit)
  → `agent/src/register.rs`; NVML/Docker snapshot collection (incl.
  `nvidia-smi -L` MIG parse) → `agent/src/inventory.rs`
- Inventory reconcile (two-phase OFFLINE/upsert, containers
  replace-all) → `controller/src/repos/inventory.rs`
- Frontend pages → `frontend/src/pages/{dashboard,deployments,servers,audit,settings,login,help-gitlab-oauth}.tsx`
- Layout shell / nav / session guard → `frontend/src/components/layout/app-shell.tsx`
- API client + query keys → `frontend/src/lib/api.ts`; hooks →
  `frontend/src/hooks/{use-auth,use-instances,use-projects}.ts`
- Dashboard sidebar tree → `frontend/src/components/containers-panel.tsx`;
  instance onboarding form → `components/instance-admin.tsx`; operator
  sign-in → `components/local-login-form.tsx`; server enrollment dialog
  + one-time command block → `components/enroll-server-dialog.tsx`;
  user menu → `components/user-menu.tsx`; shared blocks →
  `empty-state.tsx`, `slot-legend.tsx`, `mode-toggle.tsx`; shadcn
  primitives in `frontend/src/components/ui/` (generated, don't edit)
- Server hooks → `frontend/src/hooks/use-servers.ts` (10s refetch;
  detail 15s)
- Dashboard slot grid → `frontend/src/components/server-grid.tsx`
  (ServerRow/GpuStrip/SlotChip); docker-ps detail dialog →
  `components/server-detail-dialog.tsx`
- State→color map → `frontend/src/lib/states.ts`; formatting →
  `lib/format.ts`; theme + slot tokens → `frontend/src/index.css`;
  version → `frontend/src/lib/version.ts`
- Theming: `next-themes` (`ThemeProvider` in `frontend/src/main.tsx`,
  storage key `foundry-theme`, dark default)

**Planned (later phases):**

- Resource routes `servers`, `deployments`, `audit`, `agent` →
  `controller/src/routes/`
- State-machine transition functions → `controller/src/lifecycle/`
- Task queue dispatch → `controller/src/tasks/`
- Agent task executors → `agent/src/tasks/`; NVML inventory →
  `agent/src/inventory/`

## Maintenance

When modules move or appear, update this file in the same commit set. The
doc-drift hook nudges on code changes without a matching docs change.
