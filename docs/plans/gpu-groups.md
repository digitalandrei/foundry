# Plan — GPU Groups + Multi-use Slots

> **TEMPORARY PLANNING ARTIFACT.** This file is a spec to implement from in
> a later session, not living documentation. **Delete it when the feature
> ships** and fold the final design into the permanent docs
> (`ARCHITECTURE.md`, `DATABASE.md`, `API.md`, `UI-DESIGN.md`, and a
> `ROADMAP.md` phase row). See **§ Done means** at the bottom.
>
> **Status:** ⬜ Spec'd, not started (2026-06-16). Decisions locked with the
> operator (2026-06-16): **overlay** membership, **admins-only** manage
> groups *and* slot use-mode, and **multi-use slots** (GPU sharing) is now
> **in scope** alongside groups. Remaining opens in § Decisions to confirm.

## Goal

Two **independent**, operator-configured capabilities — both set in
**server config, admin-only** — that lift the current "one container, one
whole GPU" limit from opposite ends:

- **GPU groups (aggregation, 1 container : N GPUs).** A named set of whole
  GPUs on one server; deploying to it runs **one** container across all of
  them (`nvidia-smi` lists N) — multi-GPU training (DDP/FSDP/NCCL) and
  models whose VRAM exceeds a single card. The group atomically locks its
  GPUs.
- **Multi-use slots (sharing, N containers : 1 GPU).** Each slot carries a
  **single-use / multi-use** setting with a **max-occupant limit**;
  multi-use lets several containers share one GPU up to that limit. This is
  soft sharing with **no VRAM isolation** between tenants (MIG remains the
  hardware-isolated path) — an explicit, per-slot opt-in, and the editor
  warns.

The two compose under the **overlay** rule (below): they're orthogonal
axes (aggregate up vs. share down), and a group deploy is exclusive over
its members so the interaction stays well-defined.

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

**Membership semantics — overlay/template (DECIDED 2026-06-16).** A group
is a *named template* over whole GPUs. Member GPUs **remain individually
deployable** (per their own use-mode — single or multi-use) when no group
job runs. Deploying to a group requires **all members fully free** (zero
occupants) at deploy time (else rejected, naming the busy GPUs); the deploy
then locks them together. While a group deploy is RESERVED/RUNNING its
members render as occupied-by-group and are not independently deployable.
No permanently "reserved-but-idle" cards. (The exclusive alternative —
grouped GPUs *only* ever group-deployable — was rejected: it strands idle
GPUs.)

### Multi-use slots (sharing)

Orthogonal to groups. Each `gpu_slots` row gains a **max-occupant limit**
(`max_occupants`, default **1** = single-use; `>1` = multi-use). For
**individual** (non-group) deploys, a slot is deployable while its **active
occupant count < `max_occupants`**. Occupancy is just the count of active
deployments referencing that slot in `deployment_slots` — so multi-use
needs no new state machine, only a count-vs-limit check where today we
assert "FREE".

Caveats, stated plainly because they're the whole reason this is opt-in:
- **No VRAM/compute isolation** between co-tenants on a non-MIG GPU — one
  container can OOM or starve the others. MIG is the isolated way to share;
  multi-use is the soft way, and the operator owns that trade-off per slot.
- A **group deploy ignores `max_occupants`** and takes its member GPUs
  **exclusively** (a training job wants whole cards) — hence "all members
  at zero occupants" to deploy a group. Overlay + that rule make groups and
  multi-use coexist without ambiguity.

## Invariants

1. A group's members are **whole GPUs** (`gpu_slots.slot_type = FULL`),
   **MIG-disabled** (`gpus.mig_enabled = 0`), on **one server**
   (cross-server is meaningless — no NVLink/PCIe peering across hosts).
2. A GPU belongs to **at most one group** (v1 simplification —
   `UNIQUE(gpu_id)` on membership).
3. Group deploy is **all-or-nothing**: every member GPU must have **zero
   occupants**; the lock and release happen in **one transaction** over all
   members. A partial failure rolls the whole thing back.
4. Every GPU touched still gets its own slot transition + audit row (the
   trail names every card), plus the single deployment's events.
5. MIG and grouping are **mutually exclusive** on a GPU: a grouped GPU
   cannot be MIG-enabled, and a MIG-enabled GPU cannot join a group.
6. A slot hosts at most **`max_occupants`** concurrent **individual**
   deploys (default 1). Deployability checks the live count, not a binary
   FREE flag.
7. A **group deploy is exclusive** over its members — it ignores
   `max_occupants` and requires/holds the whole GPU.
8. Only-touch-managed-containers and UUID-addressed-slots invariants are
   unchanged.

## Non-goals (v1)

- **Isolated** sharing beyond what MIG already gives. Multi-use slots are
  *soft* sharing (a concurrency cap, no VRAM/compute fencing). No MPS
  orchestration, per-tenant VRAM quotas, or fractional-GPU scheduling.
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

Occupancy source of truth — generalise deployment→slot to many (this also
makes multi-use fall out for free: a slot's occupancy is just the count of
active rows pointing at it):

```sql
CREATE TABLE deployment_slots (
    deployment_id BINARY(16) NOT NULL FK deployments(id),
    gpu_slot_id   BINARY(16) NOT NULL FK gpu_slots(id),
    PRIMARY KEY (deployment_id, gpu_slot_id),
    KEY idx_deployment_slots_slot (gpu_slot_id)
);
```

Multi-use — one column on the existing slot table:

```sql
ALTER TABLE gpu_slots
    ADD COLUMN max_occupants INT UNSIGNED NOT NULL DEFAULT 1;  -- 1 = single-use
```

Inventory reconcile must **preserve** `max_occupants` (and group
membership) across re-inventory — they're operator config, not
agent-reported facts. A slot's `state` column stays for single-use
back-compat and display; for multi-use the UI shows `k / max` derived from
the active `deployment_slots` count (don't try to encode a count in the
binary state enum).

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
  (`GET/POST /api/servers/{id}/gpu-groups`, `DELETE /api/gpu-groups/{id}`),
  **admin-only** (`AdminUser` extractor). Create validates: ≥2 members
  (see § Decisions), all on the server, all FULL + MIG-disabled, none
  already grouped. Delete refused while a group deploy is live (mirror the
  volume "refused while mounted" choke point).
- **Slot use-mode** — `PATCH /api/slots/{id}` (or a per-server config
  route) sets `max_occupants`, **admin-only**. Lowering it below the
  current occupant count is allowed but does not evict — it just stops new
  deploys until tenants drain (surface that in the UI).
- **`repos/deployments.rs::create`** — replace the single "slot is FREE"
  assert with:
  - *individual deploy* — `SELECT … FOR UPDATE` the target slot, count its
    active `deployment_slots` rows, require `count < max_occupants`, then
    insert the `deployment_slots` row. (Single-use = the count-1 special
    case, so one code path.)
  - *group deploy* — `SELECT … FOR UPDATE` every member's FULL slot, assert
    **each has zero occupants**, then insert a `deployment_slots` row per
    member, all in the one create transaction.
  Reject with a precise message naming the busy/at-capacity GPUs. Keep the
  per-slot `state` transitions for single-use back-compat; for multi-use,
  occupancy is the count (the `state` enum no longer fully describes a
  shared slot).
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

- **Server config — Groups & slot use-mode** (new section on the
  server-detail page; **admin-only**, hidden for non-admins):
  - *Groups*: list, "New group" (name + multi-select of this server's FULL,
    MIG-disabled, ungrouped GPUs), delete (disabled while in use, with the
    reason). Warn on heterogeneous members.
  - *Slot use-mode*: per slot, a **single-use / multi-use** toggle and,
    when multi-use, a max-occupant number. A loud inline caveat that
    multi-use shares the GPU **without VRAM isolation** (prefer MIG when
    isolation matters).
- **Dashboard grid** ([server-grid.tsx](../../frontend/src/components/server-grid.tsx)):
  render a group as **one cell spanning its member GPUs** — header shows
  `GROUP <name> · N GPUs · <combined VRAM>` and the aggregate silicon
  telemetry (sum/мах across members); one deploy chip for the group. When
  ≥1 member is individually busy, the cell shows `unavailable — k/N busy`
  and is not a drop target. Member GPUs that are part of a group no longer
  render their own standalone slot chips (they live under the group cell).
  - A **multi-use** slot chip shows occupancy as **`k / N`**, stacks its
    occupant chips (or a compact "+2 more"), and stays a drop target while
    `k < N`.
- **Deploy paths** — drag-onto-group and the tap **slot picker**
  (`slot-picker-dialog.tsx`) list groups as targets and treat a
  not-yet-full multi-use slot as deployable; the deploy dialog's GPU/VRAM
  summary reflects N GPUs for a group. The memory-cap slider (0.31.0)
  applies per-container as today (and matters more under sharing — it caps
  each tenant).
- **`lib/slots.ts`** — extend `occupantsBySlot` to fold `deployment_slots`
  (a slot maps to a *list* of occupants); `slotDeployability` becomes
  count-based for individual deploys (deployable iff active count <
  `max_occupants`) and all-members-zero for a group target; the existing
  ONLINE/docker/external gates apply per member/slot.

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

- Group create/delete **and** slot use-mode changes: **admin-only**, both
  audited (they change what the fleet will schedule).
- Deploy authz unchanged — the requester still needs a GitLab account on
  the image's instance; `is_admin` never grants deploy.
- A group deploy writes **one** `deployment_events`/audit row for the
  deployment **plus** a slot event per member, so the trail enumerates
  every GPU locked/freed.

## Decisions

**Locked (2026-06-16):**
- **Overlay** membership (members usable individually when no group deploy
  runs).
- **Admins-only** manage groups *and* slot use-mode (`max_occupants`);
  deploy-to-group/shared-slot open to any deploy-capable user.
- **Multi-use slots in scope** — per-slot `max_occupants`, default 1
  (single-use), soft sharing with no VRAM isolation, explicit opt-in.

**Still open (confirm before coding the lifecycle):**
1. **Min group size** — recommend **≥2** (a 1-GPU "group" is just a normal
   deploy).
2. **A GPU in multiple groups** — recommend **no** for v1 (`UNIQUE(gpu_id)`).
3. **Heterogeneous members** — recommend **allow + warn**.
4. **`max_occupants` upper bound** — cap it (e.g. ≤8) so a typo can't
   oversubscribe a card into uselessness? Recommend a sane max + min 1.
5. **Mandatory per-tenant memory cap on multi-use?** Since sharing has no
   isolation, consider **requiring** the 0.31.0 memory cap (not unlimited)
   when deploying onto a multi-use slot. Recommend: warn, don't force, v1.
6. **Schema** — keep `deployments.gpu_slot_id` denormalised (recommended,
   less churn) vs fully migrate occupancy to `deployment_slots` only.
7. **Request shape** — `DeployTarget` enum vs nullable `slot_id` +
   `gpu_group_id`.

## Task breakdown (suggested order)

1. **Schema + DTOs** — migrations (`gpu_groups`, `gpu_group_members`,
   `deployment_slots`, `deployments.gpu_group_id`, `gpu_slots.max_occupants`;
   backfill `deployment_slots`), `shared` newtype + DTOs, `types.ts` mirror.
   (mysql-schema + controller + frontend)
2. **Inventory reconcile** — preserve groups **and** `max_occupants` across
   re-inventory; flag degraded on MIG-enable/member-loss. (gpu-agent +
   controller)
3. **Controller** — group CRUD + slot use-mode routes (admin-only);
   count-based + group-atomic locking in `create`; lifecycle fan-out over
   `deployment_slots`; payload multi-UUID.
4. **Agent** — widen `DeviceRequest.device_ids`; new labels; payload
   back-comp.
5. **Frontend** — groups & slot-use-mode editor (server config); grid group
   cell + multi-use `k/N` chips; deploy/slot-picker + `lib/slots.ts`.
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
- Set a slot to **multi-use, max 3**: three containers deploy onto it
  concurrently and the chip reads `3 / 3`; a fourth is refused
  (at-capacity); each shows its own CPU/mem on the chip. Lowering the limit
  to 2 blocks new deploys but doesn't evict the running three.
- A group deploy is refused while **any** member has a multi-use tenant
  (zero-occupant rule); once drained, the group deploys and holds the GPUs
  exclusively.
- Group/slot-config endpoints reject non-admins.

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
