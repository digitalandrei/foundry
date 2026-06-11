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

| Endpoint | Purpose |
|---|---|
| `GET /api/me` | Current user, linked GitLab account(s), admin flag |
| `GET /api/instances` | Onboarded GitLab instances (for login picker / browsing scope) |
| `GET /api/projects` | GitLab projects visible to the user (per instance) |
| `GET /api/registry` | Registry repositories + tags the user may deploy (browse tree: project → repository → tag) |
| `GET /api/servers` | Enrolled servers with GPUs, slots, and states |
| `GET /api/deployments` | Deployments (filterable by server/slot/state) |
| `POST /api/deployments` | Create a deployment (slot + tag + ports/env/volumes); returns the new deployment in `PENDING` |
| `POST /api/deployments/{id}/replace` | Replace flow for an occupied slot (confirmation handled in UI) |
| `POST /api/deployments/{id}/stop` · `/restart` | Lifecycle actions |
| `DELETE /api/deployments/{id}` | Remove a stopped deployment |
| `GET /api/deployments/{id}/logs` | Container logs (uploaded by agent) |
| `GET /api/audit` | Audit log (admin sees all; users see their own actions) |

Auth/OAuth endpoints (session bootstrap, not under `/api`):
`GET /auth/login/{instance}` → redirect to GitLab,
`GET /auth/callback/{instance}` → code exchange + session,
`POST /auth/logout`.

Admin-only: `POST /api/instances` (onboard a GitLab instance),
`POST /api/enrollment-tokens` (generate server enrollment token),
`POST /api/servers/{id}/rotate-token`.

## Agent API (`/agent/...`)

Authentication: agent credential issued at enrollment (agent id + secret,
sent per request; hashed at rest, rotatable). Except `/agent/enroll`, which
authenticates with a single-use enrollment token.

| Endpoint | Purpose |
|---|---|
| `POST /agent/enroll` | Exchange enrollment token for permanent agent identity; registers the server |
| `POST /agent/heartbeat` | Liveness + basic health (agent version, load); controller marks server ONLINE/OFFLINE |
| `POST /agent/inventory` | Full upload of GPUs, MIG slots, and Foundry-managed containers; controller reconciles `gpus`/`gpu_slots` |
| `GET /agent/tasks/next` | Long-poll for the next queued task for this server |
| `POST /agent/tasks/result` | Report task success/failure with detail; controller advances deployment state |
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
