# Phase 5 — Inventory (GPU/MIG Discovery & Reconciliation)

**Status:** Not started · refine this plan right before starting.

## Goal

Enrolled servers report their full GPU/MIG/container inventory; the
controller derives slots. Implements `../GPU-MIG.md`.

## Deliverables

- Agent: NVML discovery (GPUs, MIG devices, UUIDs, profiles, memory),
  Docker scan for `foundry.managed=true` containers, snapshot upload to
  `POST /agent/inventory` (on start, on `REFRESH_INVENTORY`, periodic)
- Controller: reconciliation into `gpus`/`gpu_slots` (new → FREE,
  missing → OFFLINE, UUID-keyed), server hardware metadata
- `GET /api/servers` returning servers → GPUs → slots with states
- Dashboard: server rows with GPU strips and slot chips (read-only at this
  phase), legend, system-status card (`../UI-DESIGN.md`)

## Test Host

The Phase 4 test host (read-only) is the inventory proving ground:
enumerate its real Docker containers (`foundry.managed` filtering must
show zero managed containers there) and its GPUs if present.

## Acceptance

- A MIG-enabled server shows correct slot layout in the UI; toggling MIG
  geometry on the server is reflected after refresh (removed slots OFFLINE);
  fixture tests cover A100 MIG and non-MIG layouts
