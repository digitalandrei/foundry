# Codebase Map

File routing by feature — the map of the live tree. Keep it current: when
modules move or appear, update the tables below in the same commit set (the
doc-drift hook nudges when watched code paths change without a docs change).

## Top Level

| Path | Contents | Status |
|---|---|---|
| `controller/` | `foundry-controller` binary — axum API, OAuth, scheduler, task queue, GitLab clients | live: config, /health, pool, embedded migrations |
| `agent/` | `foundry-agent` binary — task loop, Docker (bollard), NVML inventory | live: config, HTTPS client, connectivity loop |
| `shared/` | Wire contract: DTOs, state enums, ID newtypes | live |
| `frontend/` | React + TS + Vite + shadcn SPA | live: shell, theming, 11 pages |
| `migrations/` | sqlx MySQL migrations (embedded into controller, run at startup) | live: 29-table schema |
| `deployment/` | production systemd, nginx, and MariaDB backup artifacts | controller/nginx live; backup installed by next canonical deploy |
| `scripts/` | verification, deploy, and atomic local-backup automation | live |
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
  `shared/src/dto/` (operational readiness/traffic in `operations.rs`);
  frontend mirror → `frontend/src/lib/types.ts`
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
  (`mirror`: race-safe atomic GitLab project/repository/tag upserts)
  (local_admins: argon2id operator accounts; servers: enrollment
  tokens, agent identity, heartbeat, offline sweeper, guarded unused-server
  deletion). Deployment reads, target locking, and external-container adoption
  are separated into `deployment_{queries,targets,adoption}.rs`; command
  orchestration remains in `deployments.rs`.
- Agent wire-version parsing/gates → `controller/src/agent_version.rs`;
  operational deploy checks are shared by create/restart/publication paths.
- Agent auth extractor → `controller/src/auth/agent.rs`; agent routes →
  `controller/src/routes/{servers,agent}.rs`
- Routes (one module per resource) → `controller/src/routes/{health,me,instances,projects,registry}.rs`
- Bootstrap CLI (`instance add`) → `controller/src/cli.rs`
- Embedded migrations → `controller/src/main.rs` (`MIGRATOR`) reading
  `migrations/*.sql`; disposable MariaDB repository/HTTP integration tests →
  `controller/src/db_tests.rs` (ignored locally, explicit CI job)
- Agent config (TOML, `FOUNDRY_AGENT_CONFIG` override) →
  `agent/src/config.rs`; heartbeat + inventory loops, CLI dispatch →
  `agent/src/main.rs`; `--register` (enroll, self-install, user, unit)
  → `agent/src/register.rs` (host prerequisites before token consumption,
  atomic mode-0600 credential config); NVML/Docker snapshot collection (incl.
  `nvidia-smi -L` MIG parse) → `agent/src/inventory.rs`
- Live host probes + setup revision + storage accounting →
  `agent/src/host.rs`; structured nginx traffic cursor → `agent/src/traffic.rs`;
  controller ingest/metrics/retention → `controller/src/repos/traffic.rs`
- Inventory reconcile (two-phase OFFLINE/upsert, containers
  replace-all incl. ports) → `controller/src/repos/inventory.rs`
- Telemetry: agent collector (sysinfo/NVML/docker-stats, incl. per-MIG-slice
  memory) → `agent/src/metrics.rs`; series store + sweeper →
  `controller/src/repos/metrics.rs`; reusable per-server telemetry block →
  `frontend/src/components/server-telemetry.tsx` (+ `metric-sparkline.tsx`),
  shown on the server page `pages/server-detail.tsx` and the fleet-wide
  Telemetry tab `pages/telemetry.tsx`
- Frontend pages → `frontend/src/pages/{dashboard,deployments,servers,storage,audit,settings,login,help-gitlab-oauth}.tsx`;
  lazy route boundaries and Suspense fallback → `frontend/src/router.tsx`
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
  detail 15s); structured readiness/actions →
  `frontend/src/components/server-readiness.tsx`; reusable agent semver gates
  → `frontend/src/lib/agent-version.ts`
- Dashboard slot grid → `frontend/src/components/server-grid.tsx`
  (ServerRow/GpuStrip/SlotChip, with the primary mapped HTTPS URL directly
  clickable on occupied slots); docker-ps detail dialog →
  `components/server-detail-dialog.tsx`; host/service status presentation →
  `components/server-grid-status.tsx`; alert thresholds →
  `frontend/src/lib/server-alerts.ts`
- Fleet enrollment keys (list / create / delete reusable fleet tokens) →
  `frontend/src/components/fleet-keys-section.tsx` + `fleet-key-dialog.tsx`
  on the Servers page; handlers `servers::{list,create,delete}_fleet_token`
  in `controller/src/routes/servers.rs` (wired in `routes/mod.rs` under
  `/api/fleet-tokens`)
- State→color map → `frontend/src/lib/states.ts`; formatting →
  `lib/format.ts`; theme + slot tokens → `frontend/src/index.css`;
  version → `frontend/src/lib/version.ts`
- Theming: `next-themes` (`ThemeProvider` in `frontend/src/main.tsx`,
  storage key `foundry-theme`, dark default)

- State machines (transition tables + THE transition fns) →
  `controller/src/lifecycle.rs`; deployments + port allocator →
  `controller/src/repos/deployments.rs`; task queue (claim/complete/
  chains, deploy-payload build) → `controller/src/repos/tasks.rs`;
  placement/deploy-name-scoped persistent volumes + the authenticated exact
  per-server `{volume_id,path}` accounting catalog →
  `controller/src/repos/volumes.rs`; the catalog route and agent dispatch
  enrichment → `controller/src/routes/agent.rs`;
  deployment + volume routes → `controller/src/routes/volumes.rs`; live
  project authorization for images/deploy control → `controller/src/gitlab/access.rs`; policy-aware
  storage management UI → `frontend/src/pages/storage.tsx`; purge-task
  rolling-upgrade gate → `repos/volumes.rs::require_purge_support`; dispatch
  enrichment (env decrypt + pull-token mint) →
  `controller/src/routes/agent.rs`
- Volume hierarchy presentation/search →
  `frontend/src/lib/volume-locations.ts` +
  `components/{volume-location,volume-location-picker,searchable-picker,server-picker}.tsx`;
  new physical roots are allocated as
  `.foundry/{shared|slots/<id>|groups/<id>}/<deploy-name>/<mount>/<volume-id>`
  in `controller/src/repos/volumes.rs`; periodic catalog-backed usage
  measurement → `agent/src/host.rs`; symlink-safe physical-root prepare/
  validation shared by deploy, purge, browsing, accounting and deletion →
  `agent/src/file_system.rs`
- Persistent-volume files (placement protocol 0.63.0): reverse-WS session
  registry, server/deployment root selection and mutation audit → `controller/src/files.rs`;
  reverse-WS transfer loop → `agent/src/files.rs`; relative-path confinement
  + filesystem operations → `agent/src/file_system.rs`; wire protocol →
  `shared/src/dto/files.rs`; dual-pane
  UI/editor → `frontend/src/components/{volume-browser,volume-file-pane}.tsx`
  + `hooks/use-volume-files.ts` (0.59 stable upload IDs/resume offsets + quota)
- GPU groups & multi-use slots (group a GPU set, soft-share a slot among
  N containers): group CRUD + occupancy/cap + `member_slots_for_deploy`
  (FOR-UPDATE-locked member slots, counted then capped) →
  `controller/src/repos/gpu_groups.rs`; slot use-mode / shared-slot
  derivation → `controller/src/repos/slots.rs`; route →
  `controller/src/routes/gpu_groups.rs`; UI →
  `frontend/src/components/server-gpu-config.tsx`
- Agent executors (deploy/stop/restart/remove/volume orchestration) +
  task poll loop → `agent/src/tasks.rs`, which reaches Docker only
  through the `DockerEngine` seam (trait + `BollardEngine` adapter +
  test `FakeEngine`, plus pull-progress aggregation and idempotent replacement
  name handoff); the shared lazy
  `DockerRuntime` retries socket discovery without disabling operational
  tasks →
  `agent/src/docker.rs`; nginx vhost manager (HTTP/S app
  publishing: render/apply/remove, sudo-scoped reload, rollback) →
  `agent/src/vhost.rs`; host setup for it (`--setup-apps`: include +
  sudoers + TLS dir + unit) → `agent/src/register.rs`
- Registry image-config read (EXPOSE + persistent-mount/application defaults,
  compressed layer size, selected multi-arch digest→config blob)
  → `controller/src/gitlab/registry.rs`; route →
  `controller/src/routes/registry.rs` (`image_metadata`)
- Frontend deployments: hooks → `hooks/use-deployments.ts` (incl.
  useLatestMetrics + useDeploymentDetail + publication retry/traffic);
  request observability → `components/app-traffic-panel.tsx`;
  deploy/replace dialog →
  `components/deploy-dialog.tsx`, with typed field sections in
  `components/deploy-dialog-fields.tsx` and schema/defaults in
  `lib/deployment-form.ts`; tap/drag sources in
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
- Live deploy progress: agent reporter (`ProgressReporter` in
  `agent/src/tasks.rs`) + pull aggregation (`PullProgress` in
  `agent/src/docker.rs`); controller
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

- Production backup gate → `scripts/backup.sh` (atomic gzip, validation,
  root-only permissions, keep 10), `deployment/systemd/foundry-backup.*`,
  and the mandatory pre-migration step in `scripts/deploy.sh`; hermetic smoke
  test → `scripts/test-backup.sh`, real restore round-trip → CI MariaDB job

## Maintenance

When modules move or appear, update this file in the same commit set. The
doc-drift hook nudges on code changes without a matching docs change.
