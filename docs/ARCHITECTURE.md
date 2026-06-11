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
```

Any container without `foundry.managed=true` is invisible to Foundry — never
stopped, never removed, never reported as a Foundry deployment.

## Agent Tasks

The controller enqueues typed tasks; the agent polls and executes:

| Task | Effect |
|---|---|
| `DEPLOY_CONTAINER` | Pull image (with registry credentials), create container with labels + GPU device requests, start |
| `STOP_CONTAINER` | Stop a managed container |
| `RESTART_CONTAINER` | Restart a managed container |
| `REMOVE_CONTAINER` | Remove a managed container (and its resources) |
| `REFRESH_INVENTORY` | Re-enumerate GPUs/MIG slots/containers and upload |
| `UPLOAD_LOGS` | Collect and upload container logs |

Results are reported to `/agent/tasks/result`; the controller advances the
deployment state machine accordingly. Task execution must be idempotent —
the agent may re-receive a task after a crash.

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
