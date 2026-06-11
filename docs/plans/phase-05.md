# Phase 5 — Inventory (GPU/MIG Discovery & Reconciliation)

**Status:** 🔶 Built & deployed as 0.3.0 (2026-06-12) — awaiting real
snapshots from protv-ai-04-03 / -04 (operator updates the agent
binary). MIG-device enumeration via `nvidia-smi -L` (wrapper gap —
`../GPU-MIG.md`). Verified with simulated snapshots: no-MIG → FULL_GPU
slots FREE; MIG reshape → new MIG slots + old slot OFFLINE; vanished
GPU → OFFLINE; containers replace-all.

## Telemetry extension (operator request 2026-06-12 — next build)

Beyond existence, we want usage, on the dashboard (summary) and on a
**dedicated page per server**:

- **Host metrics**: CPU %, memory used/total, disk used/total (root
  mount + docker root), network rx/tx rate — `sysinfo` crate, sampled
  with each heartbeat.
- **GPU metrics**: utilization %, memory used, temperature, power —
  NVML `utilization_rates`/`memory_info` (already wrapped).
- **Container metrics**: per-container CPU/mem from the Engine API
  stats endpoint, plus **exposed/mapped ports** in `ContainerInfo`
  (bollard provides both) — feeds the Phase 6 port-publishing dialog
  prefill too.
- Transport: `POST /agent/metrics` (or piggyback on heartbeat),
  compact sample; storage: `server_metrics` ring table, 24 h retention
  + sweeper; UI: `/servers/{id}` route page with current values +
  sparklines, containers table with ports.

## Goal (original)

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
