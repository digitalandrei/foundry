# GPU & MIG Reference

How the agent discovers GPUs, derives slots, and assigns devices to
containers. Slot semantics live in `ARCHITECTURE.md` § GPU Slot Model; this
document covers the NVIDIA-specific mechanics.

## Discovery (agent side)

- Primary: **NVML** (via the `nvml-wrapper` crate) — enumerate devices,
  read UUIDs, model, memory, MIG mode, and MIG device instances.
- Fallback/cross-check: `nvidia-smi -L` and
  `nvidia-smi --query-gpu=... --format=csv` parsing. NVML is authoritative;
  nvidia-smi output is for diagnostics and sanity checks.
- Inventory runs at agent start, on `REFRESH_INVENTORY` tasks, and on a
  periodic timer; results are uploaded as a full snapshot to
  `/agent/inventory`.

## Identity Rules

- Physical GPU identity = NVML GPU UUID (`GPU-xxxxxxxx-...`).
- MIG slot identity = NVML MIG device UUID (`MIG-xxxxxxxx-...`).
- **Never** use GPU index numbers (`0`, `1`, ...) for identity or scheduling —
  they are unstable across reboots/driver updates. Display names like `0:2`
  (GPU 0, MIG slice 2) are presentation only, recomputed from current
  inventory.

## MIG Model

- A GPU with MIG mode enabled exposes MIG devices created from profiles
  (e.g. on A100 80GB: `1g.10gb`, `2g.20gb`, `3g.40gb`, `7g.80gb`).
- Foundry v1 **does not create or reshape MIG partitions** — it discovers the
  existing layout (operators manage geometry with `nvidia-smi mig`) and maps
  each MIG device to one `MIG_SLOT`.
- A GPU with MIG disabled maps to one `FULL_GPU` slot.
- If inventory shows a slot disappeared (geometry changed), the controller
  marks it `OFFLINE` and flags any deployment on it.

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
