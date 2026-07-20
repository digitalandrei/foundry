# Foundry Roadmap

Live progress tracker. **Update the status column at the end of every
phase, in the same commit set as the work.** Detailed per-phase plans live
in `docs/plans/`.

| Phase | Title | Plan | Status |
|---|---|---|---|
| 0 | Documentation & AI tooling bootstrap | (this work) | ✅ Done (2026-06-11) |
| 1 | Repository bootstrap | [plans/phase-01.md](plans/phase-01.md) | ✅ Done (2026-06-11) |
| 2 | Workspace creation | [plans/phase-02.md](plans/phase-02.md) | ✅ Done (2026-06-11) |
| 3 | Authentication (GitLab OAuth, multi-instance) | [plans/phase-03.md](plans/phase-03.md) | ✅ Done (2026-06-11) — E2E verified against g.protv.ro |
| 4 | Agent enrollment | [plans/phase-04.md](plans/phase-04.md) | ✅ Done — single-use enrollment + fleet auto-enrollment (0.42.0) live; real GPU servers enrolled (protv-ai fleet). Agent-credential rotation deferred to Phase 9 |
| 5 | Inventory (GPU/MIG discovery & reconciliation) | [plans/phase-05.md](plans/phase-05.md) | ✅ Done (2026-06-12) — inventory verified on real L40S servers (0.3/0.4); host+GPU telemetry (0.5.0), per-MIG-slice memory + fleet Telemetry tab (0.46.0) |
| 6 | Deployments (lifecycle, replacement) | [plans/phase-06.md](plans/phase-06.md) | ✅ Done — full lifecycle + replacement live on real GPU servers; persistent volumes, per-server HTTP/S publishing + EXPOSE discovery, live progress, interactive container shell (0.22.0), GPU groups + multi-use slots (0.35.0), adopt & control of external containers (0.42.0) |
| 7 | Logs | [plans/phase-07.md](plans/phase-07.md) | ✅ Done — agent push-loop capture (incremental stdout+stderr, managed only) + bounded 7-day store + UI viewer; capturing on enrolled servers |
| 8 | UI (full dashboard, dark+light themes) | [plans/phase-08.md](plans/phase-08.md) | 🔶 In progress — functional UI shipped incrementally (11 pages); route-level code splitting + keyboard interaction/DOM coverage landed in 0.51.0; Storage management landed in 0.54.0; remaining per phase-08: light-mode-complete visual sweep and empty/loading/error pass |
| 9 | Security hardening | [plans/phase-09.md](plans/phase-09.md) | ⬜ Not started — carries agent-credential rotation (deferred from Phase 4) |
| 10 | Production readiness | [plans/phase-10.md](plans/phase-10.md) | 🔶 In progress — service live; CI, audit, telemetry, structured logs, local backup automation + restore CI, dependency gates, and MariaDB integration tests in place; production timer observation, Prometheus, and load acceptance pending |
| 11 | GPU groups + multi-use slots | (retired — see Amendments 2026-06-16) | ✅ Done (0.35.0) — one container across N whole GPUs; multi-use slots soft-share a GPU among ≤4; MIG/group mutual exclusion self-heals (0.47.0) |

## Success Criteria (v1 done)

A user can:

1. Login with GitLab (any onboarded instance)
2. View authorized registries
3. View servers
4. View GPUs
5. View MIGs
6. Deploy containers (drag & drop)
7. Replace containers (with confirmation)
8. View logs
9. Audit actions
10. Operate without SSH

## Status Legend

⬜ Not started · 🔶 In progress · ✅ Done

## Amendments Log

Scope/architecture changes agreed after the original spec — each must be
reflected in the affected docs in the same commit set:

- **2026-06-23** (0.48.0) — **Agent FD-leak fix: one persistent NVML
  handle.** A regression from the 0.45–0.47 MIG work re-initialized NVML
  every collection cycle, leaking file descriptors against the NVIDIA
  driver; after ~5h an agent exhausted its FDs ("Too many open files") and
  went blind — NVML, `nvidia-smi`, even socket I/O failed. The agent now
  initializes NVML exactly once at startup (`agent/src/main.rs`) and shares
  that single handle with both the inventory and metrics ticks
  (`collect*(Option<&Nvml>)`), never re-initializing. Trade-off
  (operator-approved): a held handle does not observe a MIG layout enabled
  or reshaped *after* the agent started, so changing MIG geometry now needs
  `systemctl restart foundry-agent`; a normal boot with MIG already
  configured needs none. Affects GPU-MIG.
- **2026-06-22** (0.47.0) — **MIG/GPU-group mutual exclusion self-heals;
  "No MIG" label dropped.** GPU groups require whole, MIG-disabled members.
  If a member later has MIG enabled, inventory reconciliation
  (`apply_snapshot`) drops that membership on the next cycle and deletes any
  group it thereby empties (guarded on no live deployment) — no stale
  membership lingers. The dashboard GPU header drops the redundant "No MIG"
  text; MIG shows only by the green marker when enabled. Affects
  ARCHITECTURE (GPU groups).
- **2026-06-22** (0.46.0) — **Per-MIG-slice memory telemetry + fleet
  Telemetry tab.** The agent reports per-MIG-instance **memory**
  (used/total) via NVML MIG device handles (`nvml-wrapper` 0.12
  `mig_device_by_index`), keyed by MIG UUID in the metrics sample
  (`MetricsSample.migs`, `#[serde(default)]` for upgrade safety;
  `SlotSummary.mig_uuid` joins it). Memory only — NVML does not attribute
  utilization per slice, so the parent GPU's util still covers that. A new
  fleet-wide **Telemetry tab** (`/telemetry`) shows every enrolled server's
  host + per-GPU graphs + per-slice memory on one page; the per-server
  telemetry block was extracted into a reusable `server-telemetry.tsx`
  shared by the server-detail page and the new tab. Affects API, GPU-MIG,
  UI-DESIGN, codebase-map.
- **2026-06-22** (0.45.0) — **MIG auto-detect, stale-slot hiding,
  card.slice slot naming.** Three fixes surfaced enrolling a real split GPU
  (L40S, GPU 3 × 4 slices). Slot display names follow the layout: a full
  GPU is the bare card index (`3`), a MIG slice is `<card>.<slice>` 1-based
  (`3.1`…`3.4`). When MIG is toggled the obsolete slot lingers OFFLINE;
  `gpus_for_server` now hides an OFFLINE slot on a GPU that still has a live
  sibling (full→MIG, MIG→full, reshape orphans), while a GPU whose every
  slot is OFFLINE stays visible (it's down). The per-cycle NVML re-init this
  version introduced for runtime MIG detection was reverted in 0.48.0 (FD
  leak — see above). Affects GPU-MIG, DATABASE (slot naming).
- **2026-06-19** — **CI added (supersedes the 2026-06-11 "No CI" decision).**
  As plans get executed by other agents and Phase 10 nears, an enforced
  green gate matters more than the convenience of local-only checks (a lint
  error from 0.42.0 had already landed on `main` unnoticed). GitHub Actions
  (`.github/workflows/ci.yml`) mirrors `scripts/check.sh` on push/PR; the
  Rust job compiles against the committed sqlx offline cache (`.sqlx/`,
  `SQLX_OFFLINE=true`) so it needs no MySQL. Branch protection is the
  operator's to enable.
- **2026-06-19** (0.44.0) — **Audit-plan deliverables: N+1, lock-poison
  recovery, FE test harness, DX** (CI, the sixth deliverable, is the entry
  immediately above). Six vetted advisor-audit plans landed: the per-server
  N+1 in `servers::{list,get_summary}` collapsed to a single grouped JOIN;
  in-memory `Mutex` lock sites recover from poison via `state::lock_recover`
  instead of panicking; a Vitest + Testing-Library harness with seed tests
  for the state→color map and slot occupancy logic
  (`frontend/src/lib/{states,slots}.test.ts`, wired into `scripts/check.sh`);
  `.env.example` + a frontend `typecheck` script; and `gpu_groups`/`slots`
  module routing added to codebase-map. Affects TESTING, codebase-map.
- **2026-06-17** (0.43.0) — **Fleet keys get their own section.** Fleet
  enrollment keys moved from a one-shot modal to a managed list on the
  Servers page: multiple keys coexist (minting one no longer revokes
  others), `GET /api/fleet-tokens` lists metadata (never the raw token) and
  `DELETE /api/fleet-tokens/{id}` revokes one anytime (enrolled hosts stay
  enrolled). Admin-gated and audited. Affects API.
- **2026-06-17** (0.42.0) — **Fleet auto-enrollment + adopt & control of
  pre-running containers** (operator: agents on a launched fleet should
  self-enroll, and pre-running ComfyUI containers should be controllable
  like Foundry's own). Three additive capabilities. **Fleet enrollment** —
  a reusable, time-limited key (`enrollment_tokens.kind='FLEET'`,
  `max_uses`/`uses`, NULL `server_id`) that an agent presents via
  `--fleet-token` → `POST /agent/enroll/fleet`; the host auto-creates its
  server keyed by a now-unique `servers.hostname`, staying enrolled until
  removed. **Container mounts** — inventory now reports each container's
  volume mounts (`server_containers.mounts`), alongside the ports it
  already reported. **Adopt & control** — an operator adopts an
  externally-created container that occupies a GPU slot into a RUNNING
  deployment (`deployments.adopted_container_id`; `registry_tag_id` /
  `gitlab_instance_id` now nullable); lifecycle/shell/logs resolve it by
  docker id instead of the `foundry.managed` label, so it gets the full
  control surface (logs, console/bash, stop, delete, replace). Destructive
  ops on adopted containers are type-to-confirm and audited; the
  managed-only invariant is deliberately, auditably relaxed — never a blind
  mutation of a foreign container. The agent learns adopted ids from the
  heartbeat response. GPU telemetry was already end-to-end (phase-05) — no
  change. EC2/AMI/Terraform provisioning is explicitly out of scope (the
  operator's infra). Affects DATABASE, API, ARCHITECTURE, SECURITY,
  UI-DESIGN, preferences. Plan
  `~/.claude/plans/can-we-also-publish-snappy-hedgehog.md`.

- **2026-06-16** (0.35.0) — **GPU groups + multi-use slots** (operator:
  lift the "one container, one whole GPU" limit from both ends). Two
  independent, admin-configured capabilities. **GPU groups** — a named
  set of whole GPUs on one server (`gpu_groups`, `gpu_group_members`);
  deploying to a group runs one container across all members
  (`nvidia-smi` lists N — DDP/FSDP/NCCL, or a model exceeding one card's
  VRAM). Overlay membership: members stay individually deployable when no
  group job runs; a group deploy needs every member zero-occupant and
  locks them atomically; a GPU may overlap several groups. **Multi-use
  slots** — `gpu_slots.max_occupants` (1 = single-use, 1–4) lets several
  containers share a GPU (soft sharing, no VRAM isolation; MIG stays the
  isolated path). Both are operator/admin config and audited. New
  `deployment_slots` join table makes occupancy a single count (active
  rows per slot) and the whole lifecycle fans out over it; the agent
  builds one Docker `DeviceRequest` over all member UUIDs. `DeployTarget`
  enum (slot or group) replaces the bare `slot_id`. Decisions locked with
  the operator: overlay membership; admins-only manage groups + slot
  use-mode; size 2…all; overlap allowed; heterogeneous members fine; no
  forced per-tenant memory cap; `max_occupants` capped at 4; `gpu_slot_id`
  kept denormalised; `DeployTarget` enum. Affects DATABASE, API,
  ARCHITECTURE, UI-DESIGN. Plan `docs/plans/gpu-groups.md` retired.

- **2026-06-15** (0.27.0) — **New-image notifications** (operator: "when a
  new container image is uploaded and we're in the app, pop a message and
  show it in the listing"). While authenticated, the SPA polls
  `GET /api/registry/updates` (~90s) — a cheap **name-only** tag sync (no
  per-tag detail) across the user's available repos that returns tags
  first seen this round; the first response is a silent baseline. New tags
  raise a toast, badge the project (dot) + repo (`new`) in the sidebar
  until viewed, and invalidate the affected project's tree so an expanded
  repo shows the tag in place. Backend: `registry_tag_names` (name-only
  list factored out of `registry_tags`), `mirror::insert_new_tag_names`
  (INSERT-IGNORE new-name detector handling concurrent-tab races), a
  repos-per-poll cap. New `shared::dto::{RegistryNewTag, RegistryUpdates}`;
  frontend `RegistryWatchProvider` mounted in the app shell. Affects API,
  GITLAB-INTEGRATION, UI-DESIGN.

- **2026-06-14** (0.26.0) — **Deployment-detail action buttons**
  (operator): the deployment page header now carries the **same
  state-gated lifecycle buttons as the Deployments list** (stop ·
  re-deploy · delete · dismiss), extracted into a shared
  `DeploymentActions` component used by both; the hostname/slot sits to
  their left. Actions go through the existing mutations (shared
  `["deployments"]` query cache), so pressing one is reflected on the
  list, the detail view, and the slot grid at once. The redundant in-card
  "Clear failed deployment" button was removed. Affects UI-DESIGN.

- **2026-06-14** (0.25.0) — **Audit-log read path + deploy-auth tightening
  + tooling fixes** (audit improvement plan). **Audit Logs**: the
  append-only trail (written since Phase 6) is now readable —
  `GET /api/audit` returns a newest-first, cursor-paginated
  (`?before=&limit=`, `next_cursor`) page with an exact-match `?action=`
  filter; an admin sees every row, a non-admin only rows they are the
  actor of. The static Audit page became a query-backed table (action
  filter + Load more), realizing success criterion #9. **Deploy auth**:
  removed the `is_admin` bypass in deployment create/replace — deploying
  now requires a GitLab account on the image's instance, period; a local
  operator account (enrollment/administration only) can no longer deploy
  (matches SECURITY.md doctrine; the agent's anonymous-pull fallback is
  now only the post-create token-revoked race). **Tooling**: frontend
  `npm run lint` joined `scripts/check.sh` (confirm-dialog split into
  `confirm-context` for react-refresh); the doc-drift hook now also
  watches `lifecycle.rs` + `repos/{tasks,deployments}.rs`; fixed a stale
  "central proxy" comment (per-server nginx since 0.8.0). New
  `shared::dto::audit`. Affects API, SECURITY, codebase-map, phase-08.

- **2026-06-14** (0.22.0) — **Interactive container shell** (operator:
  "open a shell on that container … try bash and sh"). A real xterm.js
  terminal on the deployment page, built to **preserve pull-only**: the
  browser opens a WebSocket (`/api/deployments/{id}/shell`, owner/admin,
  RUNNING, audited `SHELL_OPENED`); the controller registers an in-memory
  session and the server's agent — long-polling `/agent/shell/next` —
  dials `/agent/shell/attach/{id}` back as its own WS; the controller
  bridges them and the agent `docker exec`s `bash`→`sh` (TTY, one exec)
  on the managed container. Resize + 30s keepalive pings (nginx/Cloudflare
  idle). Console/Shell box actions moved onto their title lines (Follow,
  Copy, Expand). New deps: axum `ws`, controller `futures-util`, agent
  `tokio-tungstenite` (rustls), frontend `@xterm/xterm`+`addon-fit`.
  **Needs agent redeploy (≥0.22.0)** to function. Affects API,
  ARCHITECTURE, SECURITY, DEPLOYMENT, UI-DESIGN. Realizes success
  criterion #10 (operate without SSH).

- **2026-06-14** (0.21.0) — **Deployment page full-screen 3-region
  layout** (operator). The page now uses the whole screen: Details +
  ports + mounts + env on top, then **Console and Shell side by side**
  below, each expandable to full width; on phones the three boxes stack
  one-per-viewport. The log viewer fills its panel. The **Shell box is a
  placeholder with a Start button** — the session opens only on click
  (by design); the agent-dialed reverse-WS terminal is the next change.

- **2026-06-14** (0.20.0) — **Dedicated deployment page + dashboard
  refocus + Docker-active gate** (operator). Clicking a deployment
  (dashboard slot *or* Deployments row) now opens a dedicated page
  `/deployments/{id}` (`deployment-detail.tsx`) with the details the slot
  dialog used to show **plus the live console**; the old
  `slot-detail-dialog.tsx` is retired. The dashboard's bottom Deployments
  box was removed so "Servers & GPU Slots" fills the panel (title
  de-suffixed; MIG stays a per-GPU `MIG`/`No MIG` marker). The agent now
  reports Docker daemon liveness (`servers.docker_ok`); each server shows
  a `docker: active` / `Docker stopped — deploys blocked` badge (like
  nginx) and a down daemon blocks deploys both in the UI (inert drop
  targets) and at the controller (`create` rejects). **The container
  shell was split out as a follow-up** (needs a pull-respecting reverse
  WebSocket tunnel + its own security review). Affects API, DATABASE,
  ARCHITECTURE, UI-DESIGN, phase-07/08.

- **2026-06-14** (0.19.0, Phase 7) — **Container logs + destructive-action
  confirmation** (operator). Logs: the agent ships *incremental*
  stdout+stderr for each managed running container on a 10s push loop
  (foreign containers never read), stored in a new `deployment_logs`
  table (25 tables) and served at `GET /api/deployments/{id}/logs`; the
  deployment detail dialog gained a console (merged stdout+stderr, follow
  mode, copy) and the Deployments table a console button. **Decision:
  poll-tail, not SSE; periodic push, not the `UPLOAD_LOGS` task** (a
  sequential task would block deploys). Retention is bounded twice — at
  most 7 days *and* a fixed newest-chunk count per deployment (operator:
  "keep only the last 7 days at most"); logs are deleted with the
  deployment (REMOVED) but a STOPPED deployment keeps them. Stop and
  Remove now prompt for confirmation. Affects API, ARCHITECTURE,
  DATABASE, DEPLOYMENT, phase-07.

- **2026-06-13** (0.18.0) — **Teardown reclaims container + image**
  (operator: "when we stop a container also remove the image — don't keep
  garbage"). STOP and REMOVE now delete the container (nothing lingers in
  `docker ps -a`) and reclaim its image best-effort (nothing piles up in
  `docker images`; a shared image still used by a sibling deployment is
  left untouched). Consequence: a STOPPED deployment has no container to
  start, so **restart re-deploys** — the controller's restart route calls
  `enqueue_restart` (transition `STOPPED → RESTARTING` + enqueue
  `DEPLOY_CONTAINER`), and the deploy result drives `RESTARTING →
  RUNNING`. Affects ARCHITECTURE (Deployment Lifecycle, Agent Tasks),
  container-lifecycle skill.

- **2026-06-11** — Multi-GitLab-instance support (instances onboarded
  dynamically; login per instance). Affects ARCHITECTURE, DATABASE
  (`gitlab_instances`), API, GITLAB-INTEGRATION, phase 3.
- **2026-06-11** — Original bootstrap spec retired; these docs are the
  living source of truth. Features may be added/removed/modified here.
- **2026-06-11** — UI: dark mode default per approved mockup; light mode
  required. GitLab browsing lives in the dashboard sidebar, not separate
  pages.
- **2026-06-11** (Phase 1) — Database server is **MariaDB 11.4** on this
  host, not MySQL 8.x; sqlx's MySQL driver targets it. DB `foundry` +
  scoped user provisioned (DEPLOYMENT.md § MySQL).
- **2026-06-11** (Phase 1, confirmed Phase 2) — **No CI.** Deploying is
  easy enough from this host; `scripts/check.sh` is the verification
  gate, run locally before every commit. *(Superseded 2026-06-19 — CI
  added; see top of log.)*
- **2026-06-11** (Phase 2) — Controller binds `127.0.0.1:8400` by
  default (8080 is taken on this host). Migrations are embedded in the
  controller and run at startup.
- **2026-06-11** (Phase 2) — Frontend theming via `next-themes`
  (already a shadcn/sonner dependency — reuse over a hand-rolled
  provider); storage key `foundry-theme`, dark default.
- **2026-06-11** — A separate test host (Docker, **read-only for now**)
  is available for deploying the agent against real containers — to be
  wired in during Phases 4–5 (enrollment + inventory are exactly the
  read-only surface). Connection details to be provided at Phase 4
  start.
- **2026-06-11** (Phase 3) — **OAuth over PATs**: portal-triggered
  GitLab OAuth is the only v1 login method; self-generated read-only
  PATs stay documented as a future fallback
  (GITLAB-INTEGRATION.md § Multi-Instance Model).
- **2026-06-11** (Phase 3) — **One fixed OAuth redirect URI**
  (`/auth/callback`) for all instances; pending-login state rides in an
  encrypted cookie. Replaces the spec's `/auth/callback/{instance}`.
- **2026-06-11** (Phase 3) — `sessions` table added (server-side
  sessions, hashed tokens). DATABASE.md now counts 20 tables.
- **2026-06-11** (Phase 3) — **Went live early** (user-approved; spec
  put this in Phase 10): controller systemd service + Nginx vhost +
  static SPA at `https://foundry.cloudcraft.ro`. **Serving model
  decided**: Nginx serves the frontend statically, controller is
  API-only (no rust-embed) — closes the Phase 8 decision point.
- **2026-06-11** (Phase 4) — **Version bump rule**: every production
  deploy increments the minor version (0.1 → 0.2 → …); preferences.md
  § Version sync. Deployed 0.2.0.
- **2026-06-11** (Phase 4) — **GitLab-agent-style enrollment** (user
  request): servers are created *named* in the UI, which mints the
  single-use token and prints the full
  `sudo foundry-agent --register --url … --token …` command;
  `--register` installs binary + system user + config + systemd unit
  and starts the service. `enrollment_tokens.server_id` added. Agent
  binary published at `https://foundry.cloudcraft.ro/downloads/foundry-agent`
  (glibc, Ubuntu 24.04+ — no musl build).
- **2026-06-12** (Phase 5) — **Snapshots are the truth** invariant
  (ARCHITECTURE § Invariants #5): DB observed-state is a cache;
  full-snapshot reconciliation self-heals controller/agent/container
  crashes within one interval. `server_containers` table added
  (docker-ps visibility, ALL containers with `managed` flag).
- **2026-06-12** (Phase 5) — MIG device layout parsed from
  `nvidia-smi -L` (nvml-wrapper 0.11 gap); NVML authoritative for GPUs
  + MIG mode (GPU-MIG.md).
- **2026-06-12** (0.5.0) — **Telemetry shipped** (operator request):
  host CPU/mem/disk/network + GPU util/mem/temp/power + container
  CPU/mem with port mappings, 30s samples, 24h retention; dedicated
  `/servers/{id}` page with sparklines (shadcn chart/recharts — new
  frontend dep); live System Status card. Detail dialog replaced by
  the page.
- **2026-06-12** (0.7.0, Phase 6) — **Persistent volumes** (operator):
  per-user namespaced at `/storage/containers/<owner>/<name>`,
  create-or-reuse at deploy, survive container removal, explicit
  delete via new `REMOVE_VOLUME` task type (TaskType amendment).
  Tables `server_volumes` (+ `deployment_volumes.server_volume_id`,
  `deployment_ports.kind`) — 24 tables.
- **2026-06-12** (0.7.0, Phase 6) — Deployments core shipped: lifecycle
  state machine (single transition fn + legality table + unit tests),
  task queue with long-poll dispatch, secrets/pull-token injection at
  dispatch only, result-driven replacement chain, container-crash
  reconcile via snapshots, port allocator per design, dnd drag-drop UI
  with per-port kinds (TCP/UDP now; HTTP/S blocked until the apps
  wildcard domain is chosen).
- **2026-06-12** (0.4.0) — Deterministic GPU ordering (operator):
  `gpus.display_index` persists the NVML index; lists order by it and
  UI labels use it. Natural slot-name sort (LENGTH, name).
- **2026-06-12** — **Port-publishing design for deployments** agreed:
  per-port kind chosen at drag-drop (HTTP/HTTPS via central nginx
  proxy + per-app hostname; TCP/UDP direct on server IP), controller-
  allocated non-overlapping pools, full conditions in
  plans/phase-06.md § Networking.
- **2026-06-12** (0.8.0) — **HTTP/S app publishing shipped, per-server
  model** (supersedes "central nginx" above): apps domain
  `*.ai.protv.ro`; the **agent** manages nginx vhosts on its own GPU
  server (sudoers-scoped reload, `--setup-apps`); operator wires DNS
  and places the wildcard cert at `/etc/foundry-agent/tls/` — keys
  never transit Foundry. Hostnames `<name>.ai.protv.ro` (multi-port
  `<name>-<port>`), globally unique. Deploy dialog pre-fills ports
  from the image's EXPOSE list (registry config-blob read). Containers
  pinned to their slot's device via `DeviceRequest` UUID (MIG or full
  GPU) — the API form of `docker run --gpus device=<uuid>`. Affects
  ARCHITECTURE, SECURITY, DEPLOYMENT, DATABASE, API, phase-06.
- **2026-06-12** (0.9.0) — App-publishing hardening from adversarial
  review: replacements are exempt from the hostname-uniqueness probe
  (same name → same URL across swaps; the replace dialog now prefills
  the outgoing name + ports), hostname labels validated against DNS
  rules (LDH, ≤63 chars), `deployment_ports.hostname` indexed (lock
  scope), `PortBinding.kind` serde-defaulted so pre-0.8 queued tasks
  survive upgrades, deploy-dialog form-state fixes (reset on close,
  subscribed dirty flag, host-port cleared on kind switch).
- **2026-06-12** (0.10.0) — **Live deploy progress + dashboard rework**
  (operator requests from first real deploy): agent streams pull/
  create/start progress to `POST /agent/tasks/progress` (state machine
  advances through the fine-grained states; detail text in controller
  memory — transient by design); dashboard fits the viewport with
  self-scrolling boxes (stacking below `lg`), GPU cells split the full
  row width with live silicon telemetry, slot chips carry occupant +
  live CPU/MEM (or progress while deploying) and click through to a
  detail dialog (mounts, env names, app URLs) via
  `GET /api/deployments/{id}` + `GET /api/metrics/latest`; deploy
  dialog previews the real `<name>.ai.protv.ro` hostname. Fixes from
  the failed first deploy: `--setup-apps` prepares service-user-owned
  `/storage/containers` (EROFS under the old strict unit) and re-claim
  dispatch tolerates already-advanced deployments.
- **2026-06-12** (0.11.0) — **Per-server app subdomains + slot
  auto-heal + instance management** (operator feedback): app hostnames
  are now `<name>.<server>.ai.protv.ro` (per-server wildcard DNS/cert,
  predictable routing); a *failed deploy* releases its slot to FREE
  (the agent removes any container it created; nothing is left on the
  GPU) and survives only as a FAILED deployment log — no more stuck
  slots — with `POST /api/deployments/{id}/dismiss` to clear failures
  controller-side and free a slot stuck by a stop/remove failure;
  GitLab instances are editable (URLs, secret rotation, enable/disable)
  and removable (guarded). Agent volume-create errors now point at
  `--setup-apps`; drag-drop snaps onto the slot (no fly-back). Affects
  ARCHITECTURE, API, DATABASE, DEPLOYMENT, GITLAB-INTEGRATION.
- **2026-06-12** (0.13.0) — **Server capability + external-GPU
  visibility + slot status** (operator feedback): inventory reports
  nginx/app-publishing readiness (`servers.app_publishing_ready`) — the
  UI flags a server where HTTP/S publishing would fail; the agent
  resolves each running container's GPU/MIG UUIDs
  (`server_containers.gpu_uuids`) so non-Foundry containers map onto the
  slot whose GPU they occupy (dashboard shows them, not droppable);
  slot labels follow the lifecycle vocabulary (Locked → Deploying →
  Running → Freeing), and stop/remove mark the slot Freeing
  immediately. Affects DATABASE, API, ARCHITECTURE.
- **2026-06-12** (0.14.0 / 0.15.0) — Slot occupants show a clear
  running/stopped indicator (Foundry + external; stopped external
  containers are surfaced and leave the slot droppable). Fix: a failed
  deploy now releases its host ports + app hostname too (was: slot
  freed but name/ports stayed claimed → "in use" on same-name
  redeploy). Deploy dialog shows a loader while it inspects the image
  for exposed ports.
- **2026-06-12** (0.17.0) — **Minimum nginx version enforced**
  (operator hit `unknown directive "http2"` on Ubuntu's nginx 1.24):
  the vhost template's `http2 on;` needs nginx ≥ 1.25.1, so the agent
  now parses `nginx -v` and reports NGINX_OUTDATED below that (decision:
  validate the version, keep one modern template — no compat rendering).
  New TLS_MISSING status when the operator wildcard cert isn't at
  /etc/foundry-agent/tls/ yet; `vhost::apply` preflights version + cert
  with precise errors instead of `nginx -t` emerg output; `--setup-apps`
  prints the publishing status; the misleading "is nginx installed?"
  hint now only appears on sudo refusals.
- **2026-06-12** (0.16.0) — **Granular nginx/app-publishing status**
  (operator: server has nginx but UI said "missing"): the agent now
  reports READY / NGINX_MISSING / NGINX_INACTIVE / NOT_CONFIGURED
  (binary present + `systemctl is-active nginx` + Foundry include),
  shown per server with the exact fix (and a green "nginx: active" when
  healthy). HTTP/S deploys are rejected at create on a not-ready server
  with that reason. `servers.nginx_status` column.
- **2026-06-11** (Phase 3) — First-instance bootstrap CLI:
  `foundry-controller instance add` (Settings UI requires an admin,
  who requires a login, which requires an instance).
- **2026-06-11** (Phase 3) — **Local operator accounts** (user
  request): username/argon2id-password logins (`local_credentials`,
  21 tables now) for GitLab-independent administration. CLI-managed
  (`admin add` / `admin set-password`), always `is_admin`, no GitLab
  identity → no project/registry/deploy rights. `POST /auth/local` +
  operator form on the login page. First account `admin` created on
  production.
- **2026-07-15** (0.51.0) — **Full doctrine/code audit remediation.**
  Deployment commands now commit reservation/task/event/audit atomically and
  server-side scheduling rejects running unmanaged GPU occupants; adoption is
  running-only and serialized. Fleet polling repositories batch deployments,
  GPU trees, groups, and volumes with bounded query counts. Daily plus
  pre-migration local MariaDB backups (keep 10), Rust/npm advisory gates, and
  disposable MariaDB integration CI landed. Agent registration completes host
  prerequisites before token consumption and atomically replaces its config.
  Admins may delete never-used servers only (structured dependency blockers;
  workload history is preserved). SPA routes are lazy chunks; React compiler
  warnings and audited keyboard/table navigation were fixed with DOM coverage.
  Prometheus `/metrics` is explicitly documented as pending, not live. Affects
  API, ARCHITECTURE, DEPLOYMENT, TESTING, UI-DESIGN/codebase-map.
- **2026-07-20** (0.52.0) — **Concurrent GitLab mirror refreshes are
  race-safe.** The dashboard project list and registry-update poll can fetch
  the same first-seen project simultaneously; mirror project, repository, and
  tag writes now use atomic MariaDB upserts and return the stable row selected
  by each natural unique key. A disposable-MariaDB concurrency regression test
  covers all three levels. Affects GITLAB-INTEGRATION, DATABASE, TESTING, and
  codebase-map.
- **2026-07-20** (0.53.0) — **Image-declared persistent mount defaults +
  honest registry sizes.** Dragging or tapping an image now inspects its
  selected linux/amd64 OCI config and pre-fills editable persistent mounts
  from standard Docker `VOLUME` paths or the richer
  `ai.protv.foundry.volumes` JSON label. EXPOSE ports share the same metadata
  request. Explicit zero-byte values from self-managed GitLab fall back to
  the manifest's compressed layer total; unavailable sizes are omitted
  instead of shown as `0 B`. The ComfyUI blank template declares stable
  models/output/settings/workflows volume defaults without anonymous Docker
  volumes. Affects API, ARCHITECTURE, GITLAB-INTEGRATION, DATABASE,
  UI-DESIGN, TESTING, and codebase-map.
- **2026-07-20** (0.54.0) — **Project-aware local persistent storage.**
  Volumes now combine PRIVATE/PROJECT visibility with SLOT/SERVER placement,
  use opaque host paths, support explicit ID reuse, and retain exact mounts
  across replacements. A current GitLab project member may replace a
  same-project workload in its slot; live GitLab checks remain authoritative.
  The Storage page exposes attachment state plus guarded clean/delete actions,
  and per-mount purge-on-redeploy inserts an atomic agent purge before restart
  or replacement. ComfyUI is only the first image declaring these general
  policy defaults. Affects API, ARCHITECTURE, SECURITY, DATABASE, UI-DESIGN,
  TESTING, and codebase-map.
- **2026-07-20** (0.55.0) — **Purge rolling-upgrade safety.** The enrolled
  GPU hosts still reported agent 0.48.0 when the new task type went live, so
  the controller now refuses manual clean or purge-on-redeploy until the
  target reports foundry-agent ≥0.54.0. This prevents an older agent from
  receiving an enum variant it cannot deserialize while leaving ordinary
  deploy/mount/delete operations compatible. Affects API, ARCHITECTURE,
  DEPLOYMENT, TESTING, and codebase-map.
