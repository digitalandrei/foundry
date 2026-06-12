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
| 7 | Logs | [plans/phase-07.md](plans/phase-07.md) | ⬜ Not started |
| 8 | UI (full dashboard, dark+light themes) | [plans/phase-08.md](plans/phase-08.md) | ⬜ Not started |
| 9 | Security hardening | [plans/phase-09.md](plans/phase-09.md) | ⬜ Not started |
| 10 | Production readiness | [plans/phase-10.md](plans/phase-10.md) | ⬜ Not started |

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
