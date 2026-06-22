# GPU & MIG Reference

How the agent discovers GPUs, derives slots, and assigns devices to
containers. Slot semantics live in `ARCHITECTURE.md` § GPU Slot Model; this
document covers the NVIDIA-specific mechanics.

## Discovery (agent side)

- Primary: **NVML** (via the `nvml-wrapper` crate) — enumerate devices,
  read UUIDs, model, memory, and MIG *mode*.
- **MIG device enumeration deviation (Phase 5, 2026-06-12):**
  `nvml-wrapper` 0.11 does not wrap the MIG device handles
  (`nvmlDeviceGetMigDeviceHandleByIndex`), so the per-slice layout
  (UUIDs, profiles, instance ids) is parsed from `nvidia-smi -L`
  (`agent/src/inventory.rs::parse_smi_list`, unit-tested). Slice
  memory is derived from the profile name (`1g.10gb` → 10240 MB).
  Revisit if the wrapper grows MIG support or we add raw FFI.
- Inventory runs at agent start, on `REFRESH_INVENTORY` tasks, and on a
  periodic timer; results are uploaded as a full snapshot to
  `/agent/inventory`.
- **Runtime MIG re-detection:** NVML's init/shutdown is process-ref-counted,
  and a live handle pins the GPU/MIG topology seen at init. So the agent holds
  **no** long-lived `Nvml` handle — both inventory and metrics
  (`agent/src/{inventory,metrics}.rs::collect_gpus`) call `Nvml::init()` per
  collection and drop it at scope end. This lets a MIG toggle on a running host
  show up at the next inventory cycle (≤60s) with **no agent restart**.

## Identity Rules

- Physical GPU identity = NVML GPU UUID (`GPU-xxxxxxxx-...`).
- MIG slot identity = NVML MIG device UUID (`MIG-xxxxxxxx-...`).
- **Never** use GPU index numbers (`0`, `1`, ...) for identity or scheduling —
  they are unstable across reboots/driver updates. Slot display names are
  presentation only, recomputed from current inventory: a full-GPU slot is the
  bare card index (`3`), a MIG slot is `<card>.<slice>` with the slice 1-based
  (GPU 3 split ×4 → `3.1`, `3.2`, `3.3`, `3.4`).

## MIG Model

- A GPU with MIG mode enabled exposes MIG devices created from profiles
  (e.g. on A100 80GB: `1g.10gb`, `2g.20gb`, `3g.40gb`, `7g.80gb`).
- Foundry v1 **does not create or reshape MIG partitions** — it discovers the
  existing layout (operators manage geometry with `nvidia-smi mig`) and maps
  each MIG device to one `MIG_SLOT`.
- A GPU with MIG disabled maps to one `FULL_GPU` slot.
- If inventory shows a slot disappeared (geometry changed), the controller
  marks it `OFFLINE` and flags any deployment on it.
- **Display of obsolete slots:** the OFFLINE row lingers (its `deployment_slots`
  FK has no cascade, so it is not deleted). `gpus_for_server` hides OFFLINE
  slots on a GPU that still has a live slot — the `FULL_GPU` slot left behind
  when MIG is enabled, MIG slices left when it's disabled, or stale UUIDs after
  a reshape. When *every* slot on a GPU is OFFLINE the GPU itself is down and
  they stay visible. The upsert restores the correct slot to `FREE` if the
  layout returns.

## Container GPU Assignment

Containers get GPUs via the NVIDIA Container Toolkit through the Docker
Engine API (`DeviceRequests`):

- Full GPU: device request with the GPU UUID.
- MIG slot: device request with the MIG device UUID
  (equivalent to `docker run --gpus '"device=MIG-..."'`).
- Exactly one deployment per slot — capacity is the slot itself; there is no
  oversubscription in v1.

## GPU Server Prerequisites (Ubuntu 24.04)

- NVIDIA driver (version recorded in `servers.nvidia_driver_version`)
- Docker Engine
- NVIDIA Container Toolkit configured as Docker runtime
- MIG geometry pre-created by the operator where desired

Verification commands (used by docs and the agent's preflight):
`nvidia-smi`, `nvidia-smi -L`, `nvidia-smi mig -lgi`,
`docker run --rm --gpus all nvidia/cuda:12.4.1-base-ubuntu22.04 nvidia-smi`.
