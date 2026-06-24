# Phase 5 — Inventory (GPU/MIG Discovery & Reconciliation)

**Status:** ✅ Done (2026-06-12) — inventory verified on real L40S servers
(protv-ai fleet); host+GPU telemetry shipped 0.5.0, per-MIG-slice memory +
fleet Telemetry tab 0.46.0, single persistent NVML handle 0.48.0.
MIG-device enumeration via `nvidia-smi -L` (wrapper gap — `../GPU-MIG.md`).
Reconciliation verified end-to-end: no-MIG → FULL_GPU slots FREE; MIG
reshape → new MIG slots + old slot OFFLINE (hidden while a live sibling
exists, 0.45.0); vanished GPU → OFFLINE; containers replace-all.

## Telemetry extension (operator request 2026-06-12 — ✅ shipped 0.5.0)

Delivered as designed below: agent `metrics.rs` (sysinfo host stats,
NVML GPU util/mem/temp/power, Docker stats CPU/mem per container, 30s
cadence), `ports` on the container snapshot, `POST /agent/metrics` →
`server_metrics` (24h sweeper), `GET /api/servers/{id}/metrics`,
dedicated `/servers/{id}` page (host + per-GPU sparklines via shadcn
chart/recharts, containers with CPU/mem/ports), live System Status
card. Verified by simulated ingest/range; real numbers appear once
agents update to 0.5.0.

### Original design

Beyond existence, we want usage, on the dashboard (summary) and on a
**dedicated page per server**:

- **Host metrics**: CPU %, 1-min load average + logical core count
  ("load / cores", since 0.30.0), memory used/total, disk used/total
  (root mount + docker root), network rx/tx rate — `sysinfo` crate,
  sampled with each heartbeat.
- **GPU metrics**: utilization %, memory used, temperature, power —
  NVML `utilization_rates`/`memory_info` (already wrapped).
- **Container metrics**: per-container CPU (load = cpu% / 100 over
  `online_cpus`, since 0.30.0) and mem (used/limit) from the Engine API
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
