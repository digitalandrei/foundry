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
| 4 | Agent enrollment | [plans/phase-04.md](plans/phase-04.md) | 🔶 Built & deployed (0.2.0) — awaiting first real GPU-server enrollment; rotation endpoint pending |
| 5 | Inventory (GPU/MIG discovery & reconciliation) | [plans/phase-05.md](plans/phase-05.md) | ✅ Done (2026-06-12) — inventory verified on real L40S servers (0.3/0.4); telemetry shipped (0.5.0) |
| 6 | Deployments (lifecycle, replacement) | [plans/phase-06.md](plans/phase-06.md) | 🔶 Built & deployed (0.13.0) — TCP/UDP + volumes + per-server HTTP/S publishing + EXPOSE discovery + live progress + slot auto-heal/dismiss + external-GPU visibility + nginx-readiness flag; first real GPU deploy in progress |
| 7 | Logs | [plans/phase-07.md](plans/phase-07.md) | 🔶 Built & deployed (0.19.0) — agent push-loop log capture + bounded 7-day storage + UI viewer; awaiting agent redeploy on GPU servers to start capturing |
| 8 | UI (full dashboard, dark+light themes) | [plans/phase-08.md](plans/phase-08.md) | ⬜ Not started |
| 9 | Security hardening | [plans/phase-09.md](plans/phase-09.md) | ⬜ Not started |
| 10 | Production readiness | [plans/phase-10.md](plans/phase-10.md) | ⬜ Not started |
| 11 | GPU groups + multi-use slots | [plans/gpu-groups.md](plans/gpu-groups.md) | ⬜ Spec'd (2026-06-16), not started — temporary plan, delete on ship |

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
  gate, run locally before every commit.
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
