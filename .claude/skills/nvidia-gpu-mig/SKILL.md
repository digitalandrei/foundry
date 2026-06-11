---
name: nvidia-gpu-mig
description: >
  For the Foundry project at /opt/foundry. NVIDIA GPU and MIG mechanics:
  NVML enumeration, MIG profiles and UUIDs, slot derivation, and
  inventory snapshots. Use when implementing or debugging the agent's
  inventory subsystem or slot reconciliation.
---

# NVIDIA GPU & MIG

Reference: `docs/GPU-MIG.md`. This skill covers the implementation
specifics for `agent/src/inventory/`.

## Discovery via NVML

- Use the `nvml-wrapper` crate. Init once per process; handle hosts where
  NVML is missing with a clear preflight error (agent refuses to enroll a
  non-GPU box silently).
- Per device: UUID (`GPU-...`), name/model, total memory, MIG mode
  (current + pending), and — when MIG enabled — iterate MIG device
  instances for their UUIDs (`MIG-...`), profile names (`1g.10gb`,
  `2g.20gb`, `3g.40gb`, `7g.80gb`), and memory.
- `nvidia-smi` (`-L`, `mig -lgi`) is diagnostics/cross-check only; NVML is
  authoritative.

## Slot Derivation

- MIG disabled → one `FULL_GPU` slot, identity = GPU UUID.
- MIG enabled → one `MIG_SLOT` per MIG device, identity = MIG UUID,
  `mig_profile` recorded; display name `g:i` is computed for the UI and
  carries no identity.
- Foundry v1 never creates/destroys MIG partitions — operators own
  geometry; we discover it.

## Snapshots & Reconciliation

- Inventory uploads are **full snapshots** (GPUs + slots + managed
  containers); the agent keeps no diff state.
- Controller reconciles by UUID: unknown → insert (`FREE`), missing →
  `OFFLINE`, changed metadata → update. A slot that vanishes while
  `RUNNING` flags its deployment (`docs/GPU-MIG.md` § MIG Model).
- Triggers: agent start, `REFRESH_INVENTORY` task, periodic timer.

## Container Assignment

Slot → container binding is by UUID through Docker `DeviceRequests`
(see the `docker-engine-api` skill). One deployment per slot; no
oversubscription in v1.

## Testing

Fixture-based: serialize NVML results into test fixtures for A100 MIG
layouts, non-MIG GPUs, and geometry-change cases; reconciliation tests run
against these without hardware (`docs/TESTING.md`).
