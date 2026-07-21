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

### GPU groups & multi-slot occupancy (0.35.0)

Two **independent**, admin-configured capabilities lift the "one
container, one whole GPU" limit from opposite ends. They compose: groups
aggregate up, multi-use shares down, and a group occupies the *group*
itself — not its members' individual slots — so the interaction stays
well-defined.

- **GPU groups (aggregation, 1 container : N GPUs).** A named set of whole
  GPUs on one server (`gpu_groups` + `gpu_group_members`). Deploying to a
  group runs **one** container across all members (`nvidia-smi` lists N) —
  one Docker `DeviceRequest` over every member's NVML UUID. Membership is
  **overlay**: a group occupies the *group*, not its members' own slots,
  so members stay individually deployable **even while a group container
  runs** — the operator owns any over-subscription (group occupancy is
  tracked by `gpu_group_id`, never the members' `gpu_slots.state`). A
  group itself has a **use-mode** (`gpu_groups.max_occupants`,
  1–4): single-use (one exclusive container across the GPUs — the default)
  or multi-use (the grouped GPUs shared by up to N containers, soft
  sharing — same idea as a multi-use slot, one level up). A **new** group
  deploy requires the group **below its cap** and every member **free of
  non-group holders** (a multi-use group's own concurrent deploys share
  its members; outsiders may not); single-use = exclusive among groups. A
  running group does **not** lock its members' own slots — they stay free
  and individually deployable. A GPU may be in
  **several** groups (overlap) — mutually exclusive at deploy time.
  Members are whole GPUs, MIG-disabled, on one server (no cross-host
  NVLink/PCIe peering). Group create/delete and use-mode changes are
  admin-only and audited. **MIG and grouping are mutually exclusive and
  self-healing:** the builder hides MIG cards and create rejects them, and
  if a member later has MIG enabled, inventory reconciliation drops its
  membership on the next cycle (and deletes a group it thereby empties) —
  no stale membership lingers.

- **Multi-use slots (sharing, N containers : 1 GPU).** `gpu_slots.max_occupants`
  (default 1 = single-use; 1–4) lets several containers share one GPU.
  This is **soft** sharing — **no VRAM/compute isolation** between
  co-tenants (MIG remains the hardware-isolated path); an explicit
  per-slot opt-in, and the editor warns.

**Occupancy is one count.** `deployment_slots` generalises deployment→slot
to many. A slot's live occupancy is the number of active rows pointing at
it (deployment non-terminal). That single count replaces the old "assert
the slot is FREE": an individual deploy needs `count < max_occupants`; a
group deploy needs every member's count `== 0` and writes one row per
member, all in **one transaction** (`SELECT … FOR UPDATE` over the member
slots, ordered by GPU index for a deterministic lock order). Every member
slot still gets its own state transition + the deployment's events, so the
audit trail enumerates every GPU locked/freed. The per-slot `state` enum
stays exact for single-use; for a multi-use slot it is best-effort display
(the inventory restore pass re-derives it from the most-advanced active
occupant) — deployability is the count, not the flag. The whole lifecycle
(stop/restart/remove/replace, crash/offline reconcile, dismiss) fans out
over `deployment_slots`, so a group's GPUs free atomically. Group
create/delete and slot use-mode changes are **admin-only and audited**;
deploy authz is unchanged (a GitLab account on the image's instance —
`is_admin` never grants deploy).

## Deployment Lifecycle

```
PENDING → VALIDATING → PULLING_IMAGE → CREATING_CONTAINER → STARTING
STARTING → WAITING_HEALTH → PUBLISHING → RUNNING
PUBLISHING → PUBLISH_FAILED → RUNNING  (publication-only retry)
VALIDATING → PULLING_IMAGE → PREPARED  (replacement preparation)
RUNNING → STOPPING → STOPPED
STOPPED → RESTARTING → RUNNING
STOPPED → REMOVING → REMOVED
any → FAILED
RUNNING → REPLACED   (superseded by a replacement deployment)
```

Every transition writes a `deployment_events` row (old state, new state,
actor, timestamp, detail) and is auditable end to end.

**Create-time server gates (fail fast, don't dispatch a doomed deploy):**
the target server must be ONLINE, report agent ≥0.59.0 + setup r3, and have a
fresh positive readiness set for Docker, storage and capabilities (plus
nginx/TLS for HTTP/S). The dashboard mirrors these with inert drop targets,
service badges, and the full structured readiness card. The agent repeats
preflight immediately before mutation.

**Teardown leaves no host garbage (0.18.0):** `STOP` and `REMOVE` both
delete the container (nothing lingers in `docker ps -a`) and then reclaim
its image best-effort (nothing piles up in `docker images`; an image still
referenced by a sibling deployment is left untouched). Because a STOPPED
deployment therefore has *no* container to start, **restart re-deploys**:
the restart action enqueues `DEPLOY_CONTAINER`, which re-pulls and recreates
from the stored spec, and the deploy result drives `RESTARTING → RUNNING`.
The slot stays RESERVED across stop→restart so the spec keeps its place.

**Slot auto-heal (0.11.0):** a failed *deploy* leaves nothing on the GPU
(the agent's executor removes any container it created), so the slot is
released to FREE rather than left stuck FAILED — the failure survives only
as the deployment's FAILED log. Its **host ports and app hostname are
released too** (0.15.0): a FAILED deployment with no container is excluded
from the port-allocation and hostname-uniqueness checks, so the same name
redeploys onto the freed slot. A FAILED deployment that still has a
container keeps its claims. Same for a container that vanishes from an
inventory snapshot. A STOP/REMOVE failure keeps the slot FAILED (a
container may remain); the operator clears it with **dismiss** (`Failed →
Removed`, controller-side, frees the slot). Failed deployments never hold a
slot through inventory reconciliation.

### Replacement workflow

When a user drags an image onto an **occupied** slot:

1. UI shows the current deployment and a replacement confirmation dialog.
2. User chooses Cancel or Replace.
3. The controller creates a digest-pinned successor and asks the agent to
   preflight + pull it while the old workload remains live (`PREPARE_DEPLOY`).
4. Only a successful preparation quiesces the predecessor: nginx route
   withdrawn, container stopped but deliberately retained. The successor then
   purges selected mounts, creates/starts, waits for Docker HEALTHCHECK, and
   publishes nginx.
5. Healthy + published successor → remove the retained predecessor and mark
   it REPLACED. Pull/create/start/health/publication failure → discard the
   successor and start + republish the exact retained predecessor. A workload
   that was already STOPPED before replacement stays stopped on failure.

The creator/admin may replace with any image they can read. A different
current GitLab member may replace only when the old and successor images
belong to the same project; this is resolved live against GitLab, never from
the mirror cache. Project-shared mount IDs then carry across the replacement.

## Container Labels

The agent manages **only** containers carrying Foundry labels:

```
foundry.managed=true
foundry.deployment_id=<uuid>
foundry.slot_id=<uuid>                          # primary (first) member
foundry.slot_ids=<uuid[,uuid…]>                 # all member slots (group → N)
foundry.slot=<display name, e.g. 0 or 0:3>     # hint (operator request)
foundry.gpu_uuid=<GPU-… or MIG-…>              # hint (operator request)
foundry.group_id=<uuid>                         # group deploys only
```

The two hint labels make GPU assignment visible host-side
(`docker ps --format '{{.Names}} {{.Label "foundry.gpu_uuid"}}'`);
generated container names also embed the slot (`procms-g0-x7f2`).
Identity remains `deployment_id`/`slot_id`.

Any container without `foundry.managed=true` is invisible to Foundry —
reported in inventory for visibility (ports, mounts, GPU mapping) but never
stopped, removed, or reported as a deployment **unless an operator adopts it**
(see § Adopted containers).

### Adopted containers (0.42.0 — operator requirement)

A pre-running container Foundry did **not** create (e.g. a ComfyUI image
started at boot on a fleet host) can be **adopted** into a RUNNING deployment
so it gets the full control surface — logs, console/bash, stop, delete,
replace. Because Docker labels are immutable once a container runs, an adopted
container cannot be relabeled; instead the deployment row carries
`adopted_container_id` (the docker id) and the agent resolves the target **by
id**, bypassing the `foundry.managed` label gate. Adoption requires the
container to occupy a GPU slot (resolved from its device UUIDs) — that slot
becomes the deployment's; the registry columns (`registry_tag_id`,
`gitlab_instance_id`) are NULL for adopted rows. The agent learns the set of
adopted container ids from the **heartbeat response** so its log collector
ships their logs too. The managed-only rule is thus relaxed only by an
explicit, audited operator action — never a blind mutation of a foreign
container; destructive ops are type-to-confirm in the UI (see `SECURITY.md`).

## Agent Tasks

The controller enqueues typed tasks; the agent polls and executes:

| Task | Effect |
|---|---|
| `PREPARE_DEPLOY` | Validate Docker/storage/ports/nginx candidate and pull the immutable successor without touching its predecessor |
| `DEPLOY_CONTAINER` | Repeat live preflight, pull digest-pinned image, create/start with labels + GPU/ports/volumes, wait for Docker health, publish web vhost |
| `QUIESCE_CONTAINER` | Replacement only: withdraw predecessor vhost and stop while retaining the exact container/image |
| `ROLLBACK_CONTAINER` | Start and republish the retained predecessor |
| `PUBLISH_VHOST` | Retry nginx publication for an already-healthy retained container |
| `STOP_CONTAINER` | Stop and remove a managed container, then reclaim its image (best-effort; persistent volumes survive) |
| `RESTART_CONTAINER` | Restart (running) / start (stopped) an existing managed container. Not used by the user-facing restart action, which re-deploys (see Deployment Lifecycle); retained as an executor for any directly-enqueued restart |
| `REMOVE_CONTAINER` | Remove a managed container and reclaim its image (best-effort; persistent volumes survive) |
| `REMOVE_VOLUME` | Delete a persistent volume directory (amendment: persistent storage; hard-scoped under `/storage/containers/`) |
| `PURGE_VOLUMES` | Delete and recreate an ordered batch of persistent directories before redeploy, or clean one detached volume explicitly |
| `REFRESH_INVENTORY` | Re-enumerate GPUs/MIG slots/containers and upload |
| `UPGRADE_AGENT` | Request the root-owned systemd path helper to checksum-verify, install and repair the agent host setup |
| `UPLOAD_LOGS` | (Reserved task type.) Log capture ships on a periodic **push** loop, not the task queue — see Container logs below |

### Persistent storage (amendment, Phase 6 — operator requirement)

Users can mount named local persistent volumes into containers. Every volume
belongs to one GitLab project and has two orthogonal policy axes:

- visibility `PRIVATE` (creator only) or `PROJECT` (any current project
  member);
- placement `SLOT` (the target's primary physical slot) or `SERVER` (any
  slot on that server).

The canonical identity is project + visibility scope + placement + logical
name. New host directories use opaque IDs at
`/storage/containers/volumes/<volume-uuid>`; the logical name never controls
the host path. Data is server-local and independent of deployment lifecycle:
container removal keeps it, and a permitted later deployment can select the
exact volume ID or deterministically reuse its policy key. GitLab membership
is checked live before listing/mounting project storage.

Creator/admin may explicitly **clean** a detached volume (purge contents,
retain identity) or **delete** it (purge contents and identity); both are
refused while an active deployment references it and are audited. Each
deployment binding may also opt into `purge_on_redeploy`; restart and
replacement insert one atomic `PURGE_VOLUMES` agent task before
`DEPLOY_CONTAINER`. Application-level shared-file coordination is the
workload's responsibility.

The Storage page also provides an MC-style dual-pane file browser (0.56.0).
A current project member can read/write PROJECT roots and their own PRIVATE
roots: list, folder creation, rename, same/cross-volume copy or move, recursive
delete, chunked desktop upload/download, and a bounded UTF-8 editor. The
controller performs live project authorization, registers an in-memory
session, and supplies the agent only approved `{volume_id,path}` roots. The
agent — still pull-only — discovers the session at
`/agent/volume-files/next`, dials `/agent/volume-files/attach/{id}` back, and
confines every relative path below its approved root (no absolute/traversal
paths and no symlink following). Session open plus every mutation request is
audited without file contents. Agent ≥0.56.0 is required.

Since 0.59.0, browser uploads are resumable: the browser persists a stable
upload ID, the agent keeps a partial sibling file, and `UPLOAD_READY` returns
the committed offset after reconnect. Inventory measures each opaque volume
plus filesystem total/free capacity. Creator/admin can set an advisory quota;
the browser upload path refuses a final size beyond it. The quota is not a
filesystem/container hard limit, so workloads may exceed it and the UI warns
from measured usage.

`PURGE_VOLUMES` entered the agent wire contract in 0.54.0. The controller
checks the target's heartbeat-reported agent version before accepting a purge
policy, manual clean, or enqueueing the task; older agents receive an
actionable upgrade error instead of an unknown task variant that would poison
their long-poll queue. Ordinary mounts/deploys and `REMOVE_VOLUME` remain
backward-compatible.

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

### Container logs (Phase 7 — operator requirement)

Operators read container logs from the UI without SSH. The agent runs a
10s **push** loop (same shape as metrics, *not* the sequential task
queue — which would block deploys): for each **managed** running
container (foreign containers are detected for slot visibility but never
read) it captures the new stdout+stderr since a per-deployment cursor
(`docker logs --timestamps --since`), and uploads bounded chunks to
`/agent/logs`. The controller stores them in `deployment_logs` and
serves a bounded recent window at `GET /api/deployments/{id}/logs`,
which the deployment detail dialog renders (merged console, follow mode,
copy). Retention is bounded two ways — at most 7 days *and* at most a
fixed number of newest chunks per deployment — so a chatty container
cannot exhaust the controller; logs are deleted with their deployment
(REMOVED), while a STOPPED deployment's last logs remain readable. This
is poll-tail by decision (docs/API.md § Logs design); SSE was deferred.

### Container shell (0.22.0 — operator requirement)

An in-browser terminal into a running container, **without breaking
pull-only**: the agent dials *back*. The browser opens a WebSocket
(`/api/deployments/{id}/shell`, owner/admin, RUNNING only, audited); the
controller registers an in-memory pending session; the server's agent —
already long-polling `/agent/shell/next` — dials
`/agent/shell/attach/{id}` as its own WebSocket, and the controller
bridges the two sockets verbatim. The agent runs `docker exec` with a TTY
(`bash` if present, else `sh`, in one exec) on the **managed** container
and pipes stdin/stdout/stderr through; resize is a small JSON control.
The controller never connects to the agent, there is no SSH, and the
Docker socket never leaves the server — the invariants hold. Sessions are
deliberately in-memory (a live socket pair is meaningless across a
restart); 30s keepalive pings keep nginx/Cloudflare from idling the
connection. This is the UI realization of success criterion #10 ("operate
without SSH"). Threat model: docs/SECURITY.md § Container shell.

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
  Deployment/container names are likewise unique among active workloads on
  one server, so Docker identity and the primary app label cannot be
  ambiguous; terminal history releases the name. The dashboard renders the
  primary address directly and clickably inside every occupied slot, while
  the deployment detail page renders every mapped HTTPS address.
- **Image author** (optional): `ai.protv.foundry.apps` declares one or more
  web apps (`container_port`, `scheme`, `primary`, `health_path`, body limit,
  timeout). Foundry normalizes one primary app and applies this template-owned
  policy when the image is deployed.
- **Agent**: owns `/etc/nginx/foundry-apps/<deployment_id>.conf` on its
  server — written after the container starts (80→443 redirect + TLS
  proxy_pass to `127.0.0.1:<host_port>`, websocket upgrade, streaming-
  friendly; default 2 GiB/300s, policy-overridable), removed with the
  container. Reloads
  via a sudoers rule
  scoped to `nginx -t` / `nginx -s reload`; a failing `nginx -t` rolls
  the file back. For a normal deploy, publication failure leaves the healthy
  container in recoverable `PUBLISH_FAILED` so the owner can repair nginx and
  retry publication only. During replacement it instead restores the retained
  predecessor automatically.

Host prerequisites (`foundry-agent --setup-apps`, also run by
`--register`): vhost dir + conf.d include + websocket map, a 128-byte
`server_names_hash_bucket_size` for the longer per-server app hostnames, TLS
drop point, the sudoers rule, the persistent-volume root
(`/storage/containers`, owned by the service user — first real deploy
failed without it), and the updated systemd unit (ReadWritePaths
covers it). Since 0.56.0 the unit grants only `CAP_DAC_OVERRIDE` as an
ambient agent capability so authorized file sessions can work across
arbitrary container UID ownership; the code still confines paths to
controller-approved storage roots. The capability bounding set also retains
`CAP_SETUID`, `CAP_SETGID`, and `CAP_AUDIT_WRITE` for the setuid-root `sudo`
child only, otherwise sudo cannot perform the two allowed nginx commands.
The same command is the agent **upgrade path** (reinstalls
the binary, refreshes the unit, restarts the service).

The deploy dialog pre-fills ports and persistent mounts from image metadata
(controller reads the selected linux/amd64 manifest + config blob — API.md
§ `metadata`). Standard Docker `VOLUME` paths get deterministic suggested
names; the optional `ai.protv.foundry.volumes` JSON label supplies explicit
`VolumeSpec` defaults (including visibility, placement and purge policy) for
templates that must avoid anonymous Docker volumes. Rows remain editable.
Discovery is best-effort metadata, never a deployment gate.

Deployment execution itself is not best-effort: create/replace re-fetches the
selected linux/amd64 manifest and persists `repo@sha256:<digest>`. A legacy
tag-only deployment is pinned once before its first restart. Stage-one
controller preflight requires agent ≥0.59, setup revision 3, ONLINE status and
positive live Docker/storage/capability checks (plus nginx/TLS for web apps).
Stage two runs on the target immediately before mutation: Docker reachability,
free ports, volume roots, disk headroom and an exact nginx candidate test.
Docker HEALTHCHECK is awaited for up to 180 seconds; images without one are
considered ready once running.

Every generated app vhost writes a per-deployment structured JSON access log.
The agent tails it with commit-after-ack cursors and uploads request records;
the controller retains seven days, exposes recent logs plus 24h aggregate
metrics, and deduplicates retry batches by nginx request ID. Host logrotate
keeps seven daily files and caps active files with copytruncate; deleting a
deployment route removes its host access log.

Readiness is reported, not assumed (0.13.0; granular in 0.16.0;
version + cert checks in 0.17.0): each inventory snapshot carries
`nginx_status` — READY (nginx ≥ 1.25.1 installed, the service is
active, the Foundry include and the wildcard TLS cert are present) /
NGINX_MISSING / NGINX_OUTDATED / NGINX_INACTIVE / NOT_CONFIGURED /
TLS_MISSING — stored on the server row and surfaced per server with
the exact fix. The minimum nginx version is 1.25.1 because the vhost
template uses the standalone `http2` directive (older nginx rejects it
as unknown); the agent reads `nginx -v` live, so an upgrade flips the
status without an agent restart, and `vhost::apply` re-validates
version + cert as a preflight so a stale snapshot can't sneak a deploy
into an opaque `nginx -t` emerg. An HTTP/S deploy onto a not-ready
server is **rejected at create** with that reason, rather than
dispatched only to fail on the agent.

The 0.59.0 readiness contract is structured rather than a single nginx flag.
Each 60-second snapshot (and admin-triggered diagnostic) executes Docker
socket/daemon, persistent-root write, required process capability,
`sudo -n nginx -t`, wildcard certificate coverage/expiry, and setup-revision
checks. A dedicated five-minute worker measures storage capacity and volume
usage; inventory publishes its latest completed snapshot, so traversing a
large model library can never delay heartbeats. The setup
marker is written atomically only after `--setup-apps` has installed every
revision-owned host artifact. Version text remains display/rolling-upgrade
metadata; check results are the readiness evidence.

**External GPU containers (0.13.0):** the agent resolves the GPU/MIG
UUIDs each *running* container is bound to (from its `--gpus` device
requests and `NVIDIA_VISIBLE_DEVICES`, indices mapped to UUIDs via
NVML) and reports them in inventory. The controller maps non-Foundry
containers onto the slot whose device they occupy, so the dashboard
shows externally-used GPUs (and does not offer a *running* one as a
deploy target). The same check is authoritative in the controller's locked
target-resolution transaction, so a crafted API request cannot bypass the UI.
Foundry never touches those containers, only reflects
them. Stopped containers are surfaced too (shown as stopped; the
device stays free), and every slot occupant — Foundry or external —
carries a clear running/stopped indicator.

## Server Enrollment

1. Admin generates a single-use, expiring enrollment token in the UI.
2. Operator runs `foundry-agent --register`; it validates root/config state,
   installs the binary, prepares the service user, app-publishing host pieces,
   and systemd unit, then daemon-reloads. These fallible prerequisites happen
   before token consumption.
3. Agent calls `/agent/enroll`, receives its permanent identity (agent id +
   secret), and atomically fsyncs/renames a 0600 config at
   `/etc/foundry-agent/config.toml`. `--force` keeps the previous working
   config until the replacement is durable.
4. From then on the agent authenticates every request with its identity;
   tokens are rotatable (see `SECURITY.md`).

### Fleet enrollment (0.42.0 — operator requirement)

For a launched fleet there is no per-host hand-enrollment. An admin mints a
**fleet key** (`POST /api/fleet-tokens`): a reusable, time-limited token
(`enrollment_tokens.kind='FLEET'`, optional `max_uses`, no bound server). A
host runs `foundry-agent --register --fleet-token <key>`, which calls
`/agent/enroll/fleet`; the controller **auto-creates the server** keyed by the
agent's hostname (now `UNIQUE` on `servers.hostname`) — or re-enrols an
existing one with that hostname — and issues the same permanent identity. Once
enrolled a host stays enrolled until an operator removes it. Hostnames must be
unique across the fleet (set each instance's hostname to a stable unique value,
e.g. its cloud instance id). Provisioning the hosts themselves (AMIs, cloud
fleets) is out of scope — the operator's infra.

Removal is deliberately narrow: an admin may hard-delete only a server with
no deployments (including history), volumes, GPU groups, or agent tasks. The
command removes transient inventory/telemetry/credentials in one transaction
and is audited. Any workload-bearing server remains as durable history; there
is no tombstone mode.

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

- **Metrics**: authenticated host/GPU/container telemetry flows through
  `/agent/metrics` to `/api/metrics/*`. A Prometheus-compatible `/metrics`
  endpoint is planned for Phase 10 and is not currently registered.
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
