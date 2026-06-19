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
- Audit → writes `controller/src/audit.rs` (append-only) + read API
  `controller/src/routes/audit.rs` (→ `audit::list_page`; cursor +
  `action` filter; admin sees all, non-admin actor-scoped)
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
  replace-all incl. ports) → `controller/src/repos/inventory.rs`
- Telemetry: agent collector (sysinfo/NVML/docker-stats) →
  `agent/src/metrics.rs`; series store + sweeper →
  `controller/src/repos/metrics.rs`; UI page →
  `frontend/src/pages/server-detail.tsx` + `components/metric-sparkline.tsx`
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

- State machines (transition tables + THE transition fns) →
  `controller/src/lifecycle.rs`; deployments + port allocator →
  `controller/src/repos/deployments.rs`; task queue (claim/complete/
  chains, deploy-payload build) → `controller/src/repos/tasks.rs`;
  persistent volumes → `controller/src/repos/volumes.rs`; deployment +
  volume routes → `controller/src/routes/deployments.rs`; dispatch
  enrichment (env decrypt + pull-token mint) →
  `controller/src/routes/agent.rs`
- GPU groups & multi-use slots (group a GPU set, soft-share a slot among
  N containers): group CRUD + occupancy/cap + `member_slots_for_deploy`
  (FOR-UPDATE-locked member slots, counted then capped) →
  `controller/src/repos/gpu_groups.rs`; slot use-mode / shared-slot
  derivation → `controller/src/repos/slots.rs`; route →
  `controller/src/routes/gpu_groups.rs`; UI →
  `frontend/src/components/server-gpu-config.tsx`
- Agent executors (bollard deploy/stop/restart/remove/volume) + task
  poll loop → `agent/src/tasks.rs`; nginx vhost manager (HTTP/S app
  publishing: render/apply/remove, sudo-scoped reload, rollback) →
  `agent/src/vhost.rs`; host setup for it (`--setup-apps`: include +
  sudoers + TLS dir + unit) → `agent/src/register.rs`
- Registry image-config read (EXPOSE discovery, manifest→config blob)
  → `controller/src/gitlab/registry.rs`; route →
  `controller/src/routes/registry.rs` (`exposed_ports`)
- Frontend deployments: hooks → `hooks/use-deployments.ts` (incl.
  useLatestMetrics + useDeploymentDetail); deploy/replace dialog →
  `components/deploy-dialog.tsx`; tap/drag sources in
  `containers-panel.tsx`, drop targets + live slot chips + per-server
  Docker/nginx status badges in `server-grid.tsx`; tap-to-deploy slot
  picker → `components/slot-picker-dialog.tsx` (opened via
  `components/deploy-pick-context.tsx`); shared slot occupancy +
  deploy-eligibility, one source for grid + picker → `lib/slots.ts`;
  slot/row click-through
  → dedicated deployment page (details + console) →
  `pages/deployment-detail.tsx` (route `/deployments/$deploymentId`);
  DndContext + slot grid only (deployments box removed 0.20.0) in
  `pages/dashboard.tsx`; table → `pages/deployments.tsx`
- Live deploy progress: agent reporter + pull aggregation →
  `agent/src/tasks.rs` (ProgressReporter/PullProgress); controller
  intake → `controller/src/repos/tasks.rs::progress` +
  `routes/agent.rs::tasks_progress` (detail text in
  `AppState.progress`, in-memory)

- Container logs (Phase 7): agent push-loop collector (incremental
  `docker logs --since` per managed container) → `agent/src/logs.rs`,
  uploaded in `agent/src/main.rs` heartbeat loop; controller intake
  `routes/agent.rs::logs` + bounded store/sweeper/delete →
  `controller/src/repos/logs.rs`; read route `routes/deployments.rs::logs`;
  delete-with-deployment choke point in `lifecycle.rs` (REMOVED); wire
  types → `shared/src/dto/logs.rs`; UI viewer (follow/copy) →
  `frontend/src/components/deployment-logs.tsx`, embedded in the
  deployment page `pages/deployment-detail.tsx`; hook `useDeploymentLogs`
  → `hooks/use-deployments.ts`

- Container shell (0.22.0): reverse-WS terminal. Controller bridge +
  session registry (browser WS, agent attach WS, agent long-poll) →
  `controller/src/shell.rs` (registry field on `state.rs`, routes in
  `routes/mod.rs`); agent exec bridge (long-poll → dial back → docker
  exec bash→sh TTY + resize) → `agent/src/shell.rs` (joined in
  `agent/src/main.rs`); wire type `ShellRequest` → `shared/src/dto/shell.rs`;
  UI xterm.js terminal → `frontend/src/components/shell-panel.tsx`; shared
  panel chrome → `components/detail-panel.tsx`; logs box →
  `components/console-panel.tsx`; both hosted by `pages/deployment-detail.tsx`

- Docker liveness (0.20.0): agent reports `docker_ok` (daemon answered)
  in `agent/src/inventory.rs` → `shared` `InventorySnapshot.docker_ok` →
  `servers.docker_ok` (`repos/inventory.rs`) → `ServerSummary.docker_ok`
  (`repos/servers.rs`); deploy gate in `repos/deployments.rs::create`;
  UI badge + drop-disable in `server-grid.tsx`

## Maintenance

When modules move or appear, update this file in the same commit set. The
doc-drift hook nudges on code changes without a matching docs change.
