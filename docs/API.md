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
- Pagination: `?page=` / `?per_page=` with `X-Total-Count` header.

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
| `GET /api/servers` | Servers with status/heartbeat/agent version + GPUs and slots (dashboard grid). Each server carries `app_publishing_ready` + `nginx_status` (READY / NGINX_MISSING / NGINX_INACTIVE / NOT_CONFIGURED); each slot carries `external` (a non-Foundry container occupying its GPU/MIG device, with `running`) | ✅ live |
| `GET /api/servers/{id}` | Detail: runtime versions, GPUs/slots, docker-ps container snapshot (incl. port mappings) | ✅ live |
| `GET /api/servers/{id}/metrics?minutes=N` | Telemetry series (30s samples, 24h retention; N clamped 5–1440) | ✅ live |
| `POST /api/servers` | Create a **named** server (GitLab-agent style) — returns the one-time registration command — admin | ✅ live |
| `POST /api/servers/{id}/enrollment-token` | Re-mint the token (revokes unused older ones) — admin | ✅ live |
| `GET /api/deployments` | Deployments with ports/state/uptime (REMOVED filtered out; latest 200); HTTP/S ports carry their published `hostname`; `status_detail` carries live deploy progress (in-memory overlay), `container_id` joins telemetry | ✅ live |
| `GET /api/deployments/{id}` | Detail for the slot dialog: summary (flattened) + `mounts` (volume name/host path/container path/ro) + `env` **names** (`is_secret` flagged — values never returned) | ✅ live |
| `GET /api/metrics/latest` | Newest telemetry sample per server — live GPU/container labels on the dashboard grid | ✅ live |
| `POST /api/deployments` | Create from drag-drop: slot (FREE, locked) + tag + ports (per-port kind, pool-allocated; HTTP/S get a unique `<name>.<server>.apps-domain` hostname) + env (secrets encrypted) + persistent volumes; returns it VALIDATING. HTTP/S deploys are **rejected fast** when the target server isn't publish-ready (with the nginx reason) | ✅ live |
| `POST /api/deployments/{id}/replace` | Replacement chain: stop old → remove old → REPLACED → deploy successor on the same slot | ✅ live |
| `POST /api/deployments/{id}/stop` · `/restart` | Lifecycle actions (legality enforced by the transition table) | ✅ live |
| `POST /api/deployments/{id}/dismiss` | Clear a FAILED deployment (→ REMOVED) and free its stuck slot — controller-side, no agent; owner/admin | ✅ live |
| `DELETE /api/deployments/{id}` | Remove a stopped/failed deployment (container removed; volumes survive) | ✅ live |
| `GET /api/servers/{id}/volumes` | Persistent volumes (own; admins see all) with attached-to info | ✅ live |
| `DELETE /api/volumes/{id}` | Delete volume + data (creator/admin; refused while mounted) | ✅ live |
| `GET /api/deployments/{id}/logs` | Container logs (uploaded by agent) | Phase 7 |
| `GET /api/audit` | Audit log (admin sees all; users see their own actions) | Phase 8 |
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
| `POST /agent/heartbeat` | ✅ live — marks the server ONLINE + records agent version; a 30s sweeper flips servers OFFLINE after 90s without a beat |
| `POST /agent/inventory` | ✅ live — full snapshot (GPUs/MIG + ALL containers with `managed` flag, port mappings + runtime versions) at start + every 60s; controller reconciles UUID-keyed (vanished → OFFLINE, returned → FREE), containers replace-all; bounds: ≤64 GPUs, ≤1024 containers |
| `POST /agent/metrics` | ✅ live — telemetry sample every 30s: host CPU/mem/disk/net rates (sysinfo), per-GPU util/mem/temp/power (NVML), per-container CPU/mem (Engine stats); stored as JSON in `server_metrics`, 24h sweeper |
| `GET /agent/tasks/next` | ✅ live — long-poll (≤25s server-side); DEPLOY payloads enriched at dispatch (env decrypted, pull token freshly minted — secrets never rest in the queue); lost DISPATCHED tasks re-queue after 5 min (re-claims tolerate already-advanced deployment state) |
| `POST /agent/tasks/result` | ✅ live — advances the deployment state machine; duplicate reports are idempotent no-ops; replacement chains continue here |
| `POST /agent/tasks/progress` | ✅ live — best-effort live DEPLOY progress: PULLING_IMAGE/CREATING_CONTAINER/STARTING transitions + a human detail line (`pulling: 3/7 layers · 410 / 1208 MB`, agent-throttled ~2s). Detail text is held in controller memory (transient by design); stale/out-of-order reports are dropped, never errors |
| `POST /agent/logs` | Upload container log chunks for a deployment |

Agent protocol invariants:

- Agent initiates everything; the controller never calls the agent
  (see `ARCHITECTURE.md` § Pull-Based Agent Model).
- Task execution is idempotent; the agent may receive the same task twice.
- Inventory upload is a full snapshot; the controller reconciles (slots that
  disappear go `OFFLINE`, new slots are created `FREE`).

## Observability Endpoints

- `GET /health` — liveness (no auth)
- `GET /metrics` — Prometheus metrics (bind/allowlist per `DEPLOYMENT.md`)
