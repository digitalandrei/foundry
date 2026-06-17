# Foundry API Surface

Two API families on the controller, with separate authentication. All
request/response DTOs live in the `shared` crate — the wire contract is
defined exactly once. This document tracks the surface; exact field shapes
live in `shared` once Phase 2+ lands and are mirrored here per endpoint as
they are implemented.

General rules:

- JSON request/response bodies, `serde`-serialized from `shared` types.
- Consistent error envelope: `{ "error": { "code": "...", "message": "..." } }`
  with appropriate HTTP status.
- Every state-changing endpoint writes an `audit_logs` row.
- Pagination (list endpoints that support it): cursor via
  `?before=<id>&limit=N`; the response carries `next_cursor` (null on the
  last page).

## Frontend API (`/api/...`)

Authentication: session cookie established by the GitLab OAuth flow
(HttpOnly, Secure, SameSite=Lax). Authorization: resolved against the
user's GitLab account on the instance that owns the resource.

| Endpoint | Purpose | Status |
|---|---|---|
| `GET /api/me` | Current user, linked GitLab account(s), admin flag, `apps_domain` (set → HTTP/S publishing enabled) | ✅ live |
| `GET /api/instances` | Minimal instance list `{id, name}` for the login picker — **the one unauthenticated `/api` endpoint, by design** | ✅ live |
| `GET /api/instances/full` | Full instance list (no secrets) — admin | ✅ live |
| `POST /api/instances` | Onboard a GitLab instance — admin | ✅ live |
| `PUT /api/instances/{id}` | Edit an instance (URLs, client id, optional secret rotation, enable/disable) — admin | ✅ live |
| `DELETE /api/instances/{id}` | Remove an instance — admin; refused while it has linked accounts/projects/deployments (disable instead) | ✅ live |
| `GET /api/projects` | GitLab projects visible to the user, resolved live per instance (degrades per account when an instance is unreachable) | ✅ live |
| `GET /api/registry/{project_id}` | Registry browse for one project: repositories + tags (size/pushed_at via per-tag detail, capped at 50/repo) — fetched lazily as the sidebar tree expands | ✅ live |
| `GET /api/registry/tags/{tag_id}/exposed-ports` | EXPOSE'd ports read from the image config blob (Registry v2: manifest → config; linux/amd64 picked from multi-arch indexes) — deploy-dialog prefill. Best-effort: failures return an empty list | ✅ live |
| `GET /api/registry/updates` | New-image poller: a cheap **name-only** tag sync across the user's available repos; returns tags first seen this poll → `{new_tags[]}` (`{id, tag_name, repo_path, project_id}`). The SPA polls (~90s), baselines its first response, then toasts + sidebar-badges new tags. No per-tag detail; repos-per-poll bounded | ✅ live |
| `GET /api/servers` | Servers with status/heartbeat/agent version + GPUs and slots (dashboard grid). Each server carries `app_publishing_ready` + `nginx_status` (READY / NGINX_MISSING / NGINX_OUTDATED — nginx < 1.25.1 / NGINX_INACTIVE / NOT_CONFIGURED / TLS_MISSING); each slot carries `external` (a non-Foundry container occupying its GPU/MIG device, with `running`) + `max_occupants` (multi-use cap, 1 = single-use); each GPU carries `groups` (the GPU-groups it belongs to — overlap allowed) | ✅ live |
| `GET /api/servers/{id}` | Detail: runtime versions, GPUs/slots, docker-ps container snapshot (incl. port mappings + volume mounts; non-Foundry containers carry an **Adopt** action) | ✅ live |
| `GET /api/servers/{id}/metrics?minutes=N` | Telemetry series (30s samples, 24h retention; N clamped 5–1440) | ✅ live |
| `POST /api/servers` | Create a **named** server (GitLab-agent style) — returns the one-time registration command — admin | ✅ live |
| `POST /api/servers/{id}/enrollment-token` | Re-mint the token (revokes unused older ones) — admin | ✅ live |
| `GET /api/fleet-tokens` | List live fleet keys (metadata only — id, creator, created/expires, uses/max, expired flag; never the raw token); many may coexist — admin | ✅ live (0.43.0) |
| `POST /api/fleet-tokens` | Mint a reusable fleet enrollment key `{ttl_hours, max_uses?}` → `{token, command, expires_at, max_uses}` (token shown once) — admin | ✅ live (0.42.0) |
| `DELETE /api/fleet-tokens/{id}` | Delete (revoke) a fleet key, even before it expires; enrolled hosts stay enrolled — admin | ✅ live (0.43.0) |
| `POST /api/servers/{id}/containers/{container_id}/adopt` | Adopt an external (unmanaged) container occupying a GPU slot into a RUNNING deployment → `DeploymentSummary` — admin | ✅ live (0.42.0) |
| `GET /api/deployments` | Deployments with ports/state/uptime (REMOVED filtered out; latest 200); HTTP/S ports carry their published `hostname`; `status_detail` carries live deploy progress (in-memory overlay), `container_id` joins telemetry | ✅ live |
| `GET /api/deployments/{id}` | Detail for the slot dialog: summary (flattened) + `mounts` (volume name/host path/container path/ro) + `env` **names** (`is_secret` flagged — values never returned) | ✅ live |
| `GET /api/metrics/latest` | Newest telemetry sample per server — live GPU/container labels on the dashboard grid | ✅ live |
| `POST /api/deployments` | Create from drag-drop: `target` (`{type:"slot",slot_id}` or `{type:"group",gpu_group_id}` — exactly one, locked) + tag + ports (per-port kind, pool-allocated; HTTP/S get a unique `<name>.<server>.apps-domain` hostname) + env (secrets encrypted) + persistent volumes + optional `mem_limit_mb` (Docker memory cap; clamped 32–256 GB, omitted → unlimited default); returns it VALIDATING. A slot target needs `occupants < max_occupants`; a group target needs every member at zero occupants (else rejected, naming the busy GPUs). **Rejected fast** when the target server's Docker daemon is down (`docker_ok = false`), or — for HTTP/S deploys — when it isn't publish-ready (with the nginx reason) | ✅ live |
| `POST /api/deployments/{id}/replace` | Replacement chain: stop old → remove old → REPLACED → deploy successor on the same slot(s) — re-locks the same group/slot the outgoing held | ✅ live |
| `POST /api/deployments/{id}/stop` · `/restart` | Lifecycle actions (legality enforced by the transition table) | ✅ live |
| `POST /api/deployments/{id}/dismiss` | Clear a FAILED deployment (→ REMOVED) and free its stuck slot — controller-side, no agent; owner/admin | ✅ live |
| `DELETE /api/deployments/{id}` | Remove a stopped/failed deployment (container removed; volumes survive) | ✅ live |
| `GET /api/servers/{id}/volumes` | Persistent volumes (own; admins see all) with attached-to info | ✅ live |
| `DELETE /api/volumes/{id}` | Delete volume + data (creator/admin; refused while mounted) | ✅ live |
| `GET /api/servers/{id}/gpu-groups` | GPU groups on the server → `{id, name, gpu_ids[], combined_vram_mb, max_occupants, occupants, deployable, busy_reason}[]` (deployable = below the group's cap, every member online, MIG-disabled, free of non-group holders) | ✅ live |
| `POST /api/servers/{id}/gpu-groups` | Create a group: `{name, gpu_ids[]}` (2…all FULL, MIG-disabled GPUs on the server; may overlap other groups) — single-use by default — **admin** | ✅ live |
| `DELETE /api/gpu-groups/{id}` | Delete a group — **admin**; refused while a deploy is live on it | ✅ live |
| `PATCH /api/gpu-groups/{id}` | Set a group's `max_occupants` (1–4; 1 = single-use exclusive, >1 = multi-use soft sharing of the grouped GPUs) — **admin** | ✅ live |
| `PATCH /api/slots/{id}` | Set a slot's `max_occupants` (1–4; 1 = single-use, >1 = multi-use soft sharing, no VRAM isolation) — **admin** | ✅ live |
| `GET /api/deployments/{id}/logs` | Captured container logs (merged stdout+stderr, bounded recent window) → `{content, collected_at, available}`; org-visible like the list; 404 on unknown id | ✅ live |
| `GET /api/deployments/{id}/shell` | **WebSocket** — interactive container shell (owner/admin; deployment must be RUNNING; audited `SHELL_OPENED`). The controller registers a pending session and bridges it to the server's agent (pull-only: the agent dials back). Binary frames = TTY I/O, text `{"type":"resize",cols,rows}` = resize. Closes 1011 if no agent attaches in 25s | ✅ live |
| `GET /api/audit` | Audit log, newest-first → `{entries[], next_cursor}`. Cursor `?before=<id>`, `?limit=` (1–200, default 50), `?action=` exact-match filter; `actor_name` resolved server-side. Admin sees all; a non-admin sees only rows they are the actor of | ✅ live |
| `POST /api/enrollment-tokens` | Generate server enrollment token — admin | Phase 4 |
| `POST /api/servers/{id}/rotate-token` | Rotate an agent credential — admin | Phase 4 |

Auth/OAuth endpoints (session bootstrap, not under `/api`) — ✅ live:

- `GET /auth/login/{instance_id}` → 302 to GitLab authorize (PKCE +
  CSRF state in an encrypted 10-min `foundry_oauth` cookie)
- `GET /auth/callback` → code exchange, user upsert, session cookie,
  302 to `/`. **One fixed redirect URI for all instances** (amendment:
  the spec's `/auth/callback/{instance}` was dropped — a single
  registered URI per OAuth app is simpler; the pending instance rides
  in the encrypted state cookie). Failures 302 to `/login?error=…`.
- `POST /auth/local` → local operator sign-in (`{username, password}`,
  argon2id-verified) → session cookie, 204. Failures are uniformly 401
  (no username enumeration); rate-limited by the nginx `/auth/` zone.
- `POST /auth/logout` → deletes the server-side session, clears the
  cookie, 204.

Sessions: `foundry_session` cookie — HttpOnly, Secure, SameSite=Lax,
7-day TTL, random token whose SHA-256 is stored server-side.

## Agent API (`/agent/...`)

Authentication: agent credential issued at enrollment — headers
`X-Foundry-Agent-Id: <uuid>` + `Authorization: Bearer <secret>` on every
request (secret SHA-256 at rest, constant-time compare, scoped to its
own server). Except `/agent/enroll`, which authenticates with a
single-use enrollment token.

| Endpoint | Purpose |
|---|---|
| `POST /agent/enroll` | ✅ live — single-use token → permanent identity `{agent_id, agent_secret}`; binds to the pre-named server; re-enrollment replaces the credential |
| `POST /agent/enroll/fleet` | ✅ live (0.42.0) — reusable, time-limited **fleet** key → permanent identity; auto-creates the server keyed by the agent's (unique) hostname, or re-enrolls an existing one with that hostname. Bounded by the key's TTL + `max_uses` |
| `POST /agent/heartbeat` | ✅ live — marks the server ONLINE + records agent version; a 30s sweeper flips servers OFFLINE after 90s without a beat. Response carries `adopted_containers[{container_id, deployment_id}]` so the agent ships adopted-container logs (0.42.0) |
| `POST /agent/inventory` | ✅ live — full snapshot (GPUs/MIG + ALL containers with `managed` flag, port mappings, **volume mounts** (0.42.0) + runtime versions + `docker_ok` daemon-liveness) at start + every 60s; controller reconciles UUID-keyed (vanished → OFFLINE, returned → FREE), containers replace-all; bounds: ≤64 GPUs, ≤1024 containers |
| `POST /agent/metrics` | ✅ live — telemetry sample every 30s: host CPU/load-avg/cores/mem/disk/net rates (sysinfo), per-GPU util/mem/temp/power (NVML), per-container CPU/cores/mem (Engine stats); stored as JSON in `server_metrics`, 24h sweeper |
| `POST /agent/logs` | ✅ live — container logs every 10s: a batch of *incremental* stdout+stderr chunks (`[{deployment_id, container_id, through, content}]`), one per **managed** running container (foreign containers never read); `docker logs --since` driven off a per-deployment cursor so only new output ships; each chunk authorized against its deployment+server, capped, then stored in `deployment_logs`. Bound: ≤256 chunks/batch |
| `GET /agent/tasks/next` | ✅ live — long-poll (≤25s server-side); DEPLOY payloads enriched at dispatch (env decrypted, pull token freshly minted — secrets never rest in the queue); lost DISPATCHED tasks re-queue after 5 min (re-claims tolerate already-advanced deployment state) |
| `POST /agent/tasks/result` | ✅ live — advances the deployment state machine; duplicate reports are idempotent no-ops; replacement chains continue here |
| `POST /agent/tasks/progress` | ✅ live — best-effort live DEPLOY progress: PULLING_IMAGE/CREATING_CONTAINER/STARTING transitions + a human detail line (`pulling: 3/7 layers · 410 / 1208 MB`, agent-throttled ~2s). Detail text is held in controller memory (transient by design); stale/out-of-order reports are dropped, never errors |
| `GET /agent/shell/next` | ✅ live — long-poll (≤20s); returns a pending `{session_id, deployment_id, container_id?}` shell for this server, else 204 (`container_id` set → exec an adopted container by docker id). The browser-side WS created it |
| `GET /agent/shell/attach/{session_id}` | ✅ live — **WebSocket** the agent dials back; the controller bridges it to the waiting browser. The agent `docker exec`s bash→sh (TTY) on the managed container and pipes it through. Verified `server_id` owns the session |

**Logs design (Phase 7 decision):** poll-tail, not live streaming. The
agent *pushes* incremental log chunks on a 10s loop (same shape as
`/agent/metrics`) rather than the controller enqueuing an `UPLOAD_LOGS`
task — the sequential task loop would block deploys, and a push loop
keeps the viewer continuously fresh. The UI polls `GET …/logs` every 3s
while "Follow" is on. SSE was rejected for v1: every other view already
polls, and a 3s tail is live enough. Retention is bounded twice — at
most **7 days** *and* at most a fixed number of newest chunks per
deployment (a log-spamming container is capped within one interval) —
and logs are deleted with their deployment (REMOVED), so a STOPPED
deployment's logs stay readable but a removed one's are purged.

**Shell design (reverse-WS tunnel):** an interactive shell needs a live
bidirectional channel, but the agent is pull-only — so the *agent dials
back*. The browser opens `GET /api/deployments/{id}/shell` (session
cookie auth, owner/admin, RUNNING only); the controller registers an
in-memory pending session and the server's agent — already long-polling
`/agent/shell/next` — learns of it and dials
`/agent/shell/attach/{session_id}` as its own WebSocket. The controller
then bridges the two sockets verbatim and the agent runs `docker exec`
(bash→sh, TTY) on the managed container. No inbound connection to the
agent, no SSH, no remote Docker socket — the invariant holds. Sessions
are in-memory (a live socket pair); 30s keepalive pings defeat nginx/
Cloudflare idle close; a session with no agent in 25s closes 1011.

Agent protocol invariants:

- Agent initiates everything; the controller never calls the agent
  (see `ARCHITECTURE.md` § Pull-Based Agent Model).
- Task execution is idempotent; the agent may receive the same task twice.
- Inventory upload is a full snapshot; the controller reconciles (slots that
  disappear go `OFFLINE`, new slots are created `FREE`).

## Observability Endpoints

- `GET /health` — liveness (no auth)
- `GET /metrics` — Prometheus metrics (bind/allowlist per `DEPLOYMENT.md`)
