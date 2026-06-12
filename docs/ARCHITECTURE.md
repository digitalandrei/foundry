# Foundry Architecture

Authoritative reference for system boundaries, runtime model, and invariants.
Load this when a task touches design, component boundaries, the agent protocol,
or the scheduling/lifecycle state machines. For day-to-day routing start at
`docs/ai/README.md`.

## What Foundry Is

Foundry is a self-hosted GPU orchestration platform for organizations that use
GitLab as their source of truth for authentication, authorization, source code,
CI/CD, and container registries. It deploys Docker containers from GitLab
Container Registry onto NVIDIA GPU infrastructure — full GPUs and MIG
partitions — through an explicit, drag-and-drop, slot-based scheduling model.

Foundry intentionally avoids Kubernetes. The design values:

- Simplicity and operational clarity
- Strong auditability (every state transition is recorded)
- GitLab-native workflows (no duplicated permission system)
- Explicit scheduling decisions (a human drags an image to a slot)
- Agent-based, pull-only execution
- Production-grade lifecycle management

### Non-goals (v1)

Kubernetes, multi-cluster orchestration, VM orchestration, AMD GPU support,
Windows support, SSH orchestration, complex auto-schedulers, billing.

## Component Overview

### Control plane

| Component | Role |
|---|---|
| `foundry-controller` (Rust, axum, tokio, sqlx) | API server, GitLab integration, scheduler state, task queue, audit log |
| MySQL | Sole persistent store (see `DATABASE.md`) |
| GitLab instance(s) | OAuth identity, project/registry permissions, container images |
| React frontend | SPA served behind Nginx; talks only to the controller API |

### Data plane

| Component | Role |
|---|---|
| `foundry-agent` (Rust, systemd) | Runs on each GPU server; executes tasks via local Docker Engine API |
| Docker Engine | Container runtime (local socket only; never exposed remotely) |
| NVIDIA driver stack + NVIDIA Container Toolkit | GPU access for containers |
| NVML / nvidia-smi | GPU and MIG inventory discovery |

## Pull-Based Agent Model

Inspired by the GitLab Agent architecture. **The controller never connects to
GPU servers.** All traffic is initiated by the agent:

```
foundry-agent  --HTTPS-->  foundry-controller
```

The agent periodically:

- sends heartbeats
- requests work (`/agent/tasks/next`)
- uploads GPU/MIG/container inventory
- uploads task results, status, and logs

Benefits: no inbound firewall rules on GPU servers, NAT-friendly, a single
authenticated HTTPS surface, horizontally scalable (agents are independent).

### Invariants

1. No inbound connection to a GPU server, ever. No SSH orchestration. No
   remote Docker socket.
2. The agent only manages containers it created, identified by labels
   (see § Container Labels).
3. The controller is the single source of truth for desired state; agents
   report observed state and converge toward desired state via tasks.
4. Every lifecycle transition is persisted as a `deployment_events` row and an
   `audit_logs` entry.
5. **Observed state in the DB is a cache of reality, never trusted
   across restarts** (operator requirement, 2026-06-12). Agents upload
   *full snapshots* (inventory at start + every 60 s), and the
   controller reconciles unconditionally on every upload — so a
   crashed controller, a crashed agent, a crashed container, or a MIG
   reshape all self-heal within one snapshot interval. Nothing
   incremental, no "assume the DB is right": presence in the snapshot
   is the truth, absence means gone (→ OFFLINE / removed). Phase 6
   extends the same rule to deployments: an expected-RUNNING container
   missing from the snapshot (or exited) drives the deployment state
   machine to FAILED/STOPPED — detected, evented, audited.

## Multi-GitLab-Instance Model

> Amendment to the original spec (2026-06-11): Foundry supports **one or more
> onboarded GitLab instances**, not a single hardcoded one.

- Admins onboard GitLab instances into the `gitlab_instances` table: base URL,
  registry URL, and a per-instance OAuth application (client id + secret).
  The controller must have network reachability to each onboarded instance.
- Login flow: the user picks an instance on the login page (auto-selected when
  only one is onboarded) → OAuth authorization-code flow against that
  instance → Foundry session is bound to that (instance, GitLab user).
- Permissions are resolved per instance: what the user's GitLab account can
  see on that instance is what Foundry shows. There is **no local permission
  duplication** — GitLab is the source of truth.
- Every project, registry repository, and tag row is keyed to its
  `gitlab_instance_id`. A deployment records which instance the image came
  from so the agent pulls with credentials valid for that instance.

## Authorization Model

Permissions derive entirely from GitLab. If a user can access Project A on
instance X, Foundry allows them to:

- view Project A
- view Project A's container registry
- deploy Project A's images

OAuth scopes requested: `openid`, `profile`, `email`, `read_api`,
`read_registry`. Details in `GITLAB-INTEGRATION.md`.

## GPU Slot Model

Foundry schedules onto **slots**, never raw device indexes.

Slot types:

- `FULL_GPU` — an entire physical GPU (MIG disabled or not partitioned)
- `MIG_SLOT` — one MIG instance (e.g. `1g.10gb`, `2g.20gb`, `3g.40gb`, `7g.80gb`)

Every slot has a UUID, a name, a capacity descriptor (MIG profile or full-GPU
memory), and a state. **Never rely on GPU index numbers** — indexes change
across reboots and driver updates; slots are addressed by UUID (NVML GPU/MIG
UUIDs underneath).

### Slot states

```
FREE → RESERVED → DEPLOYING → RUNNING
RUNNING → STOPPING → FREE
any → FAILED (recoverable to FREE after cleanup)
any → OFFLINE (agent lost / server down; restored on inventory)
```

| State | Meaning | UI color |
|---|---|---|
| `FREE` | Available for deployment | green |
| `RESERVED` | Claimed by a pending deployment, not yet running | yellow |
| `DEPLOYING` | Agent is pulling/creating/starting | yellow (animated) |
| `RUNNING` | Workload active | blue |
| `FAILED` | Last operation failed; needs cleanup/retry | red |
| `STOPPING` | Stop in progress | yellow |
| `OFFLINE` | Server/agent unreachable | gray |

(Color semantics mirror `UI-DESIGN.md`; keep the two documents in sync.)

## Deployment Lifecycle

```
PENDING → VALIDATING → PULLING_IMAGE → CREATING_CONTAINER → STARTING → RUNNING
RUNNING → STOPPING → STOPPED
STOPPED → RESTARTING → RUNNING
STOPPED → REMOVING → REMOVED
any → FAILED
RUNNING → REPLACED   (superseded by a replacement deployment)
```

Every transition writes a `deployment_events` row (old state, new state,
actor, timestamp, detail) and is auditable end to end.

**Slot auto-heal (0.11.0):** a failed *deploy* leaves nothing on the GPU
(the agent's executor removes any container it created), so the slot is
released to FREE rather than left stuck FAILED — the failure survives only
as the deployment's FAILED log. Same for a container that vanishes from an
inventory snapshot. A STOP/REMOVE failure keeps the slot FAILED (a
container may remain); the operator clears it with **dismiss** (`Failed →
Removed`, controller-side, frees the slot). Failed deployments never hold a
slot through inventory reconciliation.

### Replacement workflow

When a user drags an image onto an **occupied** slot:

1. UI shows the current deployment and a replacement confirmation dialog.
2. User chooses Cancel or Replace.
3. Replace executes in order: stop old → remove old → pull new → start new.
   The old deployment ends in state `REPLACED`; the new one follows the normal
   lifecycle. Both are linked in the audit trail.

## Container Labels

The agent manages **only** containers carrying Foundry labels:

```
foundry.managed=true
foundry.deployment_id=<uuid>
foundry.slot_id=<uuid>
foundry.slot=<display name, e.g. 0 or 0:3>     # hint (operator request)
foundry.gpu_uuid=<GPU-… or MIG-…>              # hint (operator request)
```

The two hint labels make GPU assignment visible host-side
(`docker ps --format '{{.Names}} {{.Label "foundry.gpu_uuid"}}'`);
generated container names also embed the slot (`procms-g0-x7f2`).
Identity remains `deployment_id`/`slot_id`.

Any container without `foundry.managed=true` is invisible to Foundry — never
stopped, never removed, never reported as a Foundry deployment.

## Agent Tasks

The controller enqueues typed tasks; the agent polls and executes:

| Task | Effect |
|---|---|
| `DEPLOY_CONTAINER` | Pull image (with registry credentials), create container with labels + GPU device requests + port bindings + volume binds, start |
| `STOP_CONTAINER` | Stop a managed container |
| `RESTART_CONTAINER` | Restart (running) / start (stopped) a managed container |
| `REMOVE_CONTAINER` | Remove a managed container (persistent volumes survive) |
| `REMOVE_VOLUME` | Delete a persistent volume directory (amendment: persistent storage; hard-scoped under `/storage/containers/`) |
| `REFRESH_INVENTORY` | Re-enumerate GPUs/MIG slots/containers and upload |
| `UPLOAD_LOGS` | Collect and upload container logs |

### Persistent storage (amendment, Phase 6 — operator requirement)

Users can mount named persistent volumes into containers. Volumes are
per-server, **per-user namespaced** host directories at
`/storage/containers/<owner>/<name>`, created on first use at deploy
time, and independent of any deployment's lifecycle: removing a
container keeps the data, and the same volume can be mounted into a
later container (including the successor in a replacement). Deletion
is explicit (UI/API), refused while any active deployment mounts the
volume, and wipes the directory via `REMOVE_VOLUME`. Users see and
mount only their own volumes; admins see all.

Results are reported to `/agent/tasks/result`; the controller advances the
deployment state machine accordingly. Task execution must be idempotent —
the agent may re-receive a task after a crash.

DEPLOY tasks additionally stream **live progress** to
`/agent/tasks/progress` (0.10.0): stage transitions (PULLING_IMAGE →
CREATING_CONTAINER → STARTING via the same transition table) plus a
throttled human detail line aggregated from Docker's pull stream
(`pulling: 3/7 layers · 410 / 1208 MB`). Progress is best-effort by
contract: posts are fire-and-forget on the agent, stale/out-of-order
reports are dropped by the controller, and the detail text lives in
controller memory only — the durable truth stays the state machine.

## App Publishing (amendment, 0.8.0 — operator requirement)

HTTP/S ports are published as per-app hostnames under a wildcard apps
domain (`*.ai.protv.ro`) instead of raw host ports; TCP/UDP keep
mapping directly onto the server IP.

Division of labor (operator decision — no Cloudflare/DNS integration):

- **Operator (once, per server)**: wildcard DNS `*.<server>.ai.protv.ro`
  pointing at that GPU server, and a wildcard certificate for
  `*.<server>.ai.protv.ro` dropped at
  `/etc/foundry-agent/tls/{fullchain.pem,privkey.pem}` on it (renewals
  too — private keys never travel through Foundry).
- **Controller**: assigns hostnames at create time as a **per-server
  subdomain** — `<name>.<server>.<domain>`, or
  `<name>-<port>.<server>.<domain>` for several web ports — unique across
  all active deployments (`deployment_ports.hostname`), and ships them in
  the DEPLOY payload. The per-server subdomain gives predictable DNS and
  lets the operator issue one wildcard cert per server
  (`*.<server>.<domain>`). Enabled by `FOUNDRY_APPS_DOMAIN`; unset rejects
  HTTP/S kinds. A replacement keeps its predecessor's hostname.
- **Agent**: owns `/etc/nginx/foundry-apps/<deployment_id>.conf` on its
  server — written after the container starts (80→443 redirect + TLS
  proxy_pass to `127.0.0.1:<host_port>`, websocket upgrade, streaming-
  friendly), removed with the container. Reloads via a sudoers rule
  scoped to `nginx -t` / `nginx -s reload`; a failing `nginx -t` rolls
  the file back. A vhost that cannot be published fails the deploy and
  tears the container down — the URL is part of the contract.

Host prerequisites (`foundry-agent --setup-apps`, also run by
`--register`): vhost dir + conf.d include + websocket map, TLS drop
point, the sudoers rule, the persistent-volume root
(`/storage/containers`, owned by the service user — first real deploy
failed without it), and the updated systemd unit (ReadWritePaths
covers it). The same command is the agent **upgrade path** (reinstalls
the binary, refreshes the unit, restarts the service).

The deploy dialog pre-fills ports from the image's EXPOSE list
(controller reads the registry config blob — API.md
§ `exposed-ports`); discovery is best-effort metadata, never a gate.

Readiness is reported, not assumed (0.13.0): each inventory snapshot
carries `app_publishing` (nginx + the Foundry include present), stored
on the server row and surfaced in the UI — a server without nginx is
flagged before a deploy fails on it.

**External GPU containers (0.13.0):** the agent resolves the GPU/MIG
UUIDs each *running* container is bound to (from its `--gpus` device
requests and `NVIDIA_VISIBLE_DEVICES`, indices mapped to UUIDs via
NVML) and reports them in inventory. The controller maps non-Foundry
containers onto the slot whose device they occupy, so the dashboard
shows externally-used GPUs (and does not offer a *running* one as a
deploy target) — Foundry never touches those containers, only reflects
them. Stopped containers are surfaced too (shown as stopped; the
device stays free), and every slot occupant — Foundry or external —
carries a clear running/stopped indicator.

## Server Enrollment

1. Admin generates a single-use, expiring enrollment token in the UI.
2. Operator installs `foundry-agent` on the GPU server and runs enrollment
   with the token.
3. Agent calls `/agent/enroll`, receives its permanent identity (agent id +
   secret), and stores config at `/etc/foundry-agent/config.toml`.
4. From then on the agent authenticates every request with its identity;
   tokens are rotatable (see `SECURITY.md`).

## API Surface

Two distinct API families with distinct authentication (full contract in
`API.md`):

- **Frontend API** (`/api/...`) — session-authenticated (GitLab OAuth):
  `/api/me`, `/api/projects`, `/api/registry`, `/api/servers`,
  `/api/deployments`
- **Agent API** (`/agent/...`) — agent-credential-authenticated:
  `/agent/enroll`, `/agent/heartbeat`, `/agent/inventory`,
  `/agent/tasks/next`, `/agent/tasks/result`, `/agent/logs`

## Workspace Layout (planned)

Cargo workspace with three Rust crates plus the frontend:

```
controller/   # foundry-controller binary (axum API, scheduler, GitLab clients)
agent/        # foundry-agent binary (task loop, Docker, NVML)
shared/       # DTOs, API models, enums, common validation — the wire contract
frontend/     # React + TypeScript + Vite + shadcn/ui SPA
migrations/   # sqlx MySQL migrations
deployment/   # systemd units, nginx vhost, install scripts
scripts/      # dev/CI helpers
```

`shared` is the contract between controller, agent, and (via generated or
mirrored types) the frontend. Enums like slot state, deployment state, and
task type live there exactly once.

### Code organization rule: no god files

No module may become a dumping ground. Keep modules small and
single-responsibility; extract shared logic into `shared` (Rust) or
`frontend/src/lib` + composed components (React) instead of duplicating it.
Reuse existing code and styles before writing new ones. This applies to
docs too — each document owns one topic.

## Observability

- **Metrics**: Prometheus-compatible `/metrics` on the controller; agent
  metrics flow through heartbeat/inventory.
- **Logging**: structured JSON via `tracing` in both controller and agent.
- **Tracing**: OpenTelemetry-ready (instrumented spans; exporter optional).

## Technology Stack

| Layer | Choice |
|---|---|
| Controller | Rust, axum, tokio, sqlx (MySQL), reqwest, serde, tracing, OAuth2 |
| Agent | Rust, tokio, reqwest (HTTPS only), Docker Engine API (bollard), NVML + nvidia-smi, systemd |
| Frontend | React, TypeScript, Vite, shadcn/ui, TanStack Query, TanStack Router, dnd-kit |
| Database | MySQL (sqlx migrations) |
| OS | Ubuntu 24.04 (controller and GPU servers) |
| GPU servers prerequisites | NVIDIA drivers, Docker, NVIDIA Container Toolkit |

## Related Documents

- `DATABASE.md` — schema
- `API.md` — endpoint contracts
- `GITLAB-INTEGRATION.md` — OAuth, API, registry
- `GPU-MIG.md` — discovery and slot derivation
- `SECURITY.md` — threat model and controls
- `DEPLOYMENT.md` — production setup on this host
- `UI-DESIGN.md` — dashboard design, theming
- `ROADMAP.md` + `plans/` — phase tracking
