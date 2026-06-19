# Plan 002: Route the GPU-groups + multi-use-slot modules in codebase-map

> **Executor instructions**: Follow this plan step by step. Run every
> verification command. If a STOP condition occurs, stop and report. When done,
> update this plan's row in `advisor-plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 14f9d95..HEAD -- docs/ai/codebase-map.md controller/src/repos/gpu_groups.rs controller/src/repos/slots.rs`
> If `codebase-map.md` changed since this plan was written, re-check whether the
> modules below are already routed before adding them.

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: docs
- **Planned at**: commit `14f9d95`, 2026-06-19

## Why this matters

`docs/ai/codebase-map.md` is the file-routing layer every agent loads before
touching code, and Foundry doctrine is explicit: **"docs are the spec ... must
not drift"** and "when modules move or appear, update this file in the same
commit set" (`codebase-map.md` § Maintenance; `docs/ai/preferences.md`). Two
shipped backend modules are not routed anywhere in the map:

- `controller/src/repos/gpu_groups.rs` (435 lines) — GPU group creation,
  occupancy/cap tracking, `member_slots_for_deploy`, multi-slot occupant
  aggregation. Landed with the GPU-groups feature.
- `controller/src/repos/slots.rs` — slot use-mode / shared-slot derivation.

An agent asked to fix group occupancy or slot-sharing logic today has to grep
blind. This is small but it is exactly the drift the doctrine forbids, and the
doc-drift Stop hook is informational only, so it slipped.

## Current state

`docs/ai/codebase-map.md` § "Feature → Location" has a sub-bullet block that
ends with the state-machine/deployment routing. Searching the file:

```
$ grep -n 'gpu_groups\|repos/slots' docs/ai/codebase-map.md
(no matches)
```

The related modules that ARE routed give the house style to match — e.g. the
deployments/lifecycle bullet:

```
- State machines (transition tables + THE transition fns) →
  `controller/src/lifecycle.rs`; deployments + port allocator →
  `controller/src/repos/deployments.rs`; task queue ... →
  `controller/src/repos/tasks.rs`; persistent volumes →
  `controller/src/repos/volumes.rs`; ...
```

Confirmed present (do NOT re-add — they appear in grouped `{...}` notation):
`instances.rs`, `users.rs`, `mirror.rs`, `local_admins.rs`, `servers.rs`.

Related already-shipped surfaces for the same feature, to reference in the new
entry: route module `controller/src/routes/gpu_groups.rs`; frontend
`frontend/src/components/server-gpu-config.tsx`; migrations
`...0002_gpu_groups.sql` and `...0003_group_use_mode.sql`.

## Commands you will need

| Purpose | Command | Expected |
|---------|---------|----------|
| Confirm modules exist | `ls controller/src/repos/gpu_groups.rs controller/src/repos/slots.rs` | both listed |
| Confirm drift | `grep -c 'gpu_groups' docs/ai/codebase-map.md` | `0` before, `≥1` after |

(No build needed — this is a docs-only change.)

## Scope

**In scope**:
- `docs/ai/codebase-map.md` (add routing for the two modules)

**Out of scope**:
- Any code file — this plan changes documentation only.
- Re-describing the whole GPU-groups feature — one accurate routing bullet is
  the deliverable, not a design doc. The design lives in `docs/GPU-MIG.md` /
  `docs/ARCHITECTURE.md`.

## Git workflow

- Branch: `advisor/002-codebase-map-drift`
- One commit: `docs: route gpu_groups + slots modules in codebase-map`
- Do NOT push unless instructed.

## Steps

### Step 1: Add a GPU-groups routing bullet

In `docs/ai/codebase-map.md` § "Feature → Location", add a bullet near the
state-machine/deployment entries, matching the existing `name → \`path\``
style. It must name both modules and their role, e.g.:

> - GPU groups & multi-use slots (group a GPU set, soft-share a slot among N
>   containers) → group CRUD + occupancy/cap + `member_slots_for_deploy`
>   (FOR-UPDATE-locked member slots) → `controller/src/repos/gpu_groups.rs`;
>   slot use-mode / shared-slot derivation → `controller/src/repos/slots.rs`;
>   route → `controller/src/routes/gpu_groups.rs`; UI →
>   `frontend/src/components/server-gpu-config.tsx`.

### Step 2: Verify the drift is closed

**Verify**: `grep -n 'gpu_groups.rs' docs/ai/codebase-map.md` → at least one
match that points at `controller/src/repos/gpu_groups.rs`, and
`grep -n 'repos/slots.rs' docs/ai/codebase-map.md` → one match.

## Done criteria

- [ ] `grep -c 'gpu_groups.rs' docs/ai/codebase-map.md` ≥ 1
- [ ] `grep -c 'repos/slots.rs' docs/ai/codebase-map.md` ≥ 1
- [ ] `git diff --stat` shows only `docs/ai/codebase-map.md` changed
- [ ] `advisor-plans/README.md` status row updated

## STOP conditions

- The modules no longer exist at those paths (feature was removed/renamed) —
  report the rename instead of routing a dead path.
- The map already routes them (a later commit fixed it) — mark this plan
  REJECTED ("fixed independently") in the index.

## Maintenance notes

- The root cause is process, not content: module-add commits don't reliably
  update the map because the drift hook only nudges. If the team wants this to
  stop recurring, that's a DX change (a check that every `repos/*.rs` is named
  in the map) — out of scope here, note it for the backlog.
