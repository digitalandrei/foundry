# Plan — GPU Groups (multi-GPU containers)

> **TEMPORARY PLANNING ARTIFACT.** This file is a spec to implement from in
> a later session, not living documentation. **Delete it when the feature
> ships** and fold the final design into the permanent docs
> (`ARCHITECTURE.md`, `DATABASE.md`, `API.md`, `UI-DESIGN.md`, and a
> `ROADMAP.md` phase row). See **§ Done means** at the bottom.
>
> **Status:** ⬜ Spec'd, not started (2026-06-16). Open decisions in
> § Decisions to confirm must be answered before coding the lifecycle.

## Goal

Let one deployment occupy **multiple whole GPUs on one server** so the
container sees all of them (`nvidia-smi` lists N) — the missing capability
for multi-GPU training (DDP/FSDP/NCCL) and models whose VRAM exceeds a
single card. Operators **define named GPU groups in server config**; a
group is then a one-click deploy target that atomically locks its GPUs.

Scope is **aggregation** (1 container : N GPUs). It is *not* GPU sharing
(N containers : 1 GPU) — that stays MIG's job (hardware-isolated slices,
already modelled). See § Non-goals.

## The model decision

Today a deployment binds **exactly one** `gpu_slots` row
(`deployments.gpu_slot_id`, NOT NULL) and the slot state machine enforces
one occupant per slot (`FREE → RESERVED → DEPLOYING → RUNNING → …`).

A group deploy is therefore, mechanically, **a deployment that occupies
several slots at once**. We reuse the existing per-slot occupancy machine
rather than inventing a parallel lock: a group deploy locks the
`slot_type = FULL` slot of every member GPU, all pointing at the same
deployment. The "group" is the durable, named set that (a) gives the UI a
single deploy target and (b) constrains which GPU combinations are valid.

**Recommended membership semantics — overlay/template (not exclusive):**
a group is a *named template* over whole GPUs. Member GPUs remain
individually deployable when no group job runs. Deploying to a group
requires **all members FREE** at deploy time (else rejected with which
GPUs are busy); the deploy then locks them together. While a group deploy
is RESERVED/RUNNING, its members render as occupied-by-group and are not
independently deployable. This maximises flexibility (GPUs usable solo or
as a group) with no permanently "reserved-but-idle" cards.

The alternative — **exclusive** membership (grouped GPUs are *only* ever
group-deployable, never individual) — is simpler to reason about but
strands GPUs whenever the group is idle. **Recommend overlay; confirm in
§ Decisions.**

## Invariants

1. A group's members are **whole GPUs** (`gpu_slots.slot_type = FULL`),
   **MIG-disabled** (`gpus.mig_enabled = 0`), on **one server**
   (cross-server is meaningless — no NVLink/PCIe peering across hosts).
2. A GPU belongs to **at most one group** (v1 simplification —
   `UNIQUE(gpu_id)` on membership).
3. Group deploy is **all-or-nothing**: every member slot must be FREE; the
   lock and release happen in **one transaction** over all members. A
   partial failure rolls the whole thing back.
4. Every GPU touched still gets its own slot transition + audit row (the
   trail names every card), plus the single deployment's events.
5. MIG and grouping are **mutually exclusive** on a GPU: a grouped GPU
   cannot be MIG-enabled, and a MIG-enabled GPU cannot join a group.
6. Only-touch-managed-containers and UUID-addressed-slots invariants are
   unchanged.

## Non-goals (v1)

- GPU sharing without partitioning (use MIG).
- Cross-server groups / multi-node jobs (no NCCL-over-network
  orchestration).
- Topology-aware placement (NVLink pairing heuristics) — Docker exposes
  the devices; NCCL discovers topology. We may *warn* on heterogeneous
  members but won't optimise.
- Fractional/among-group scheduling, gang queueing (see Deploy queue,
  a separate idea).

## Data model

New tables:

```sql
CREATE TABLE gpu_groups (
    id         BINARY(16)  PK,
    server_id  BINARY(16)  NOT NULL FK servers(id),
    name       VARCHAR(64) NOT NULL,
    created_by BINARY(16)  NOT NULL FK users(id),
    created_at DATETIME(6) NOT NULL,
    updated_at DATETIME(6) NOT NULL,
    UNIQUE KEY uq_gpu_groups_name (server_id, name)
);

CREATE TABLE gpu_group_members (
    group_id BINARY(16) NOT NULL FK gpu_groups(id) ON DELETE CASCADE,
    gpu_id   BINARY(16) NOT NULL FK gpus(id),
    PRIMARY KEY (group_id, gpu_id),
    UNIQUE KEY uq_member_gpu (gpu_id)          -- one group per GPU (inv. 2)
);
```

Occupancy source of truth — generalise deployment→slot to many:

```sql
CREATE TABLE deployment_slots (
    deployment_id BINARY(16) NOT NULL FK deployments(id),
    gpu_slot_id   BINARY(16) NOT NULL FK gpu_slots(id),
    PRIMARY KEY (deployment_id, gpu_slot_id),
    KEY idx_deployment_slots_slot (gpu_slot_id)
);
```

`deployments` gains `gpu_group_id BINARY(16) NULL FK gpu_groups(id)` (NULL
= single-GPU deploy). Keep `deployments.gpu_slot_id` as the **denormalised
primary slot** (first/only member) so existing single-slot queries and the
deployment-detail UI keep working with minimal churn; `deployment_slots`
is authoritative for occupancy. A single-GPU deploy writes one
`deployment_slots` row and `gpu_group_id = NULL` — uniform handling.

**Migration note:** backfill `deployment_slots` from existing
`deployments.gpu_slot_id` for active rows in the same migration so
occupancy queries can switch over atomically.

## Controller changes

- **Group CRUD** — new `repos/gpu_groups.rs` + routes
  (`GET/POST /api/servers/{id}/gpu-groups`, `DELETE /api/gpu-groups/{id}`).
  Create validates: ≥2 members (see § Decisions), all on the server, all
  FULL + MIG-disabled, none already grouped. Delete refused while any
  member slot is non-FREE under a live group deploy (mirror the
  volume "refused while mounted" choke point).
- **`repos/deployments.rs::create`** — when the request targets a group:
  `SELECT … FOR UPDATE` every member's FULL slot, assert all FREE, then
  transition each FREE→RESERVED and insert one `deployment_slots` row per
  member, all in the existing create transaction. Reuse
  `lifecycle::transition_slot` per slot (the deployment side is unchanged —
  still one `transition_deployment`). Reject with a precise message naming
  the busy GPUs.
- **Lifecycle fan-out** — stop/restart/remove/replace and the
  crash/offline reconcile must iterate `deployment_slots` and move **all**
  member slots together (today they touch the single `gpu_slot_id`). Add a
  helper `member_slots(deployment_id)` and route every slot transition
  through it.
- **Task payload build (`repos/tasks.rs::enqueue_deploy`)** — emit all
  member GPU device UUIDs (see Agent/Shared).
- **Replacement** — unchanged shape; the new deployment locks the same
  member slots the outgoing one held.

## Agent changes

- `DeployPayload.gpu_device_uuid: String` → add
  `gpu_device_uuids: Vec<String>` (1 or N). The executor builds a single
  `DeviceRequest { device_ids: Some(gpu_device_uuids), … }`
  ([agent/src/tasks.rs](../../agent/src/tasks.rs) ~L422-430 already builds
  this — just widen the vec). For rollback safety across one release, keep
  populating the singular field (first UUID) and have the agent prefer the
  vec, falling back to `[gpu_device_uuid]` when the vec is absent
  (`#[serde(default)]`).
- Container labels: add `foundry.group_id` and `foundry.slot_ids`
  (comma-joined) alongside the existing `foundry.slot_id` (= primary).
- No NVLink/topology handling — exposing the devices is enough; NCCL does
  the rest. Document this.

## Shared DTO changes

- `dto/deployment.rs::CreateDeploymentRequest` — `slot_id: SlotId` becomes
  "either a slot or a group" target. Cleanest: add
  `target: DeployTarget` enum `{ Slot(SlotId), Group(GpuGroupId) }`, or
  keep `slot_id` optional + add `gpu_group_id: Option<GpuGroupId>`
  (exactly one set; validated). New `GpuGroupId` newtype in `shared/ids`.
- New `dto` types: `GpuGroup { id, name, gpu_ids, server_id, … }`,
  `GpuGroupSummary` (for the grid: member count, combined VRAM, a
  `deployable`/`busy_reason`), `CreateGpuGroupRequest { name, gpu_ids }`.
- `DeployPayload` multi-UUID field (above).
- Mirror all of the above in `frontend/src/lib/types.ts`.

## Frontend changes

- **Server config — Group editor** (new section on the server-detail page
  or settings): list groups, "New group" (name + multi-select of this
  server's FULL, MIG-disabled, ungrouped GPUs), delete (disabled while in
  use with the reason). Admin-gated (see § Decisions).
- **Dashboard grid** ([server-grid.tsx](../../frontend/src/components/server-grid.tsx)):
  render a group as **one cell spanning its member GPUs** — header shows
  `GROUP <name> · N GPUs · <combined VRAM>` and the aggregate silicon
  telemetry (sum/мах across members); one deploy chip for the group. When
  ≥1 member is individually busy, the cell shows `unavailable — k/N busy`
  and is not a drop target. Member GPUs that are part of a group no longer
  render their own standalone slot chips (they live under the group cell).
- **Deploy paths** — drag-onto-group and the tap **slot picker**
  (`slot-picker-dialog.tsx`) list groups as targets; the deploy dialog's
  GPU/VRAM summary reflects N GPUs. The memory-cap slider (0.31.0) applies
  per-container as today.
- **`lib/slots.ts`** — extend `occupantsBySlot` to fold
  `deployment_slots`, and `slotDeployability` to cover a group target
  (deployable iff all members FREE; the existing ONLINE/docker/external
  gates apply per member).

## Edge cases & failure modes

- **Member offline / vanished from inventory** while a group deploy runs →
  reconcile the deployment to FAILED and free the surviving members (same
  policy as a single slot going OFFLINE under a live deploy). An undeployed
  group with an offline member is simply not deployable.
- **MIG enabled on a member** → inventory reconcile flags the group
  **degraded**; deploys blocked until the operator removes the GPU from the
  group or disables MIG. Never silently re-slice.
- **External (non-Foundry) container** holding a member GPU → group not
  deployable (reuse the external-holder check already in `slotDeployability`).
- **Delete group while a deploy is live on it** → refused.
- **Partial lock contention** (a member taken between SELECT and lock) →
  `FOR UPDATE` + the all-FREE assert inside one tx makes it all-or-nothing.
- **Group of 1** → degenerate single-GPU deploy; require ≥2 (§ Decisions).
- **Heterogeneous members** (mixed models/VRAM) → allowed, warn in the
  editor (NCCL prefers homogeneous).

## Security & audit

- Group create/delete: audited; admin/owner-gated (§ Decisions).
- Deploy authz unchanged — the requester still needs a GitLab account on
  the image's instance; `is_admin` never grants deploy.
- A group deploy writes **one** `deployment_events`/audit row for the
  deployment **plus** a slot event per member, so the trail enumerates
  every GPU locked/freed.

## Decisions to confirm (before coding the lifecycle)

1. **Membership: overlay vs exclusive.** Recommend **overlay** (members
   usable individually when no group deploy is active). ← biggest call.
2. **Min group size** — recommend **≥2** (a 1-GPU "group" is just a normal
   deploy).
3. **Who manages groups** — recommend **admins** (server config is an
   operator concern); deploy-to-group open to any deploy-capable user.
4. **A GPU in multiple groups** — recommend **no** for v1 (`UNIQUE(gpu_id)`).
5. **Heterogeneous members** — recommend **allow + warn**.
6. **Schema** — keep `deployments.gpu_slot_id` denormalised (recommended,
   less churn) vs fully migrate occupancy to `deployment_slots` only.
7. **Request shape** — `DeployTarget` enum vs nullable `slot_id` +
   `gpu_group_id`.

## Task breakdown (suggested order)

1. **Schema + DTOs** — migrations (`gpu_groups`, `gpu_group_members`,
   `deployment_slots`, `deployments.gpu_group_id`; backfill), `shared`
   newtype + DTOs, `types.ts` mirror. (mysql-schema + controller + frontend)
2. **Inventory reconcile** — keep groups intact across re-inventory; flag
   degraded on MIG-enable/member-loss. (gpu-agent + controller)
3. **Controller** — group CRUD repo+routes; multi-slot atomic lock in
   `create`; lifecycle fan-out over `deployment_slots`; payload multi-UUID.
4. **Agent** — widen `DeviceRequest.device_ids`; new labels; payload
   back-comp.
5. **Frontend** — group editor; grid group cell; deploy/slot-picker +
   `lib/slots.ts`.
6. **Docs fold-in + delete this plan** (§ Done means).

## Acceptance

- Create a 2-GPU group on a real server; deploy an image that sees **both**
  GPUs (`nvidia-smi` lists 2) and reaches RUNNING.
- Both member GPUs show **occupied-by-group**; neither is independently
  deployable while the group runs; stop/remove frees **both** atomically.
- Every member GPU has slot events and the deployment has its
  events/audit; replacement re-locks the same members.
- Deleting a busy group is refused; a member going OFFLINE degrades the
  group and blocks (or fails) the deploy; MIG-enabling a member degrades
  the group.

## Done means (cleanup — required)

When shipped:

1. **Delete this file** (`docs/plans/gpu-groups.md`).
2. Fold the final design into the permanent docs in the same commit set:
   `DATABASE.md` (new tables + `deployments` columns), `API.md` (group
   CRUD + deploy-target shape), `ARCHITECTURE.md` (multi-slot occupancy +
   the atomic group-lock rule), `UI-DESIGN.md` (group editor + grid group
   cell).
3. Add a `ROADMAP.md` phase row (or Amendments entry) marking it Done with
   the shipped version, and bump the version per the deploy rule.
