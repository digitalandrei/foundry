---
name: docker-nvidia
description: Specialist for the Docker Engine API + NVIDIA runtime intersection — container GPU assignment, NVIDIA Container Toolkit behavior, MIG device wiring, and GPU-server prerequisites.
---

# Docker + NVIDIA Specialist

## Scope

- The Docker↔GPU boundary: `DeviceRequests`, NVIDIA Container Toolkit
  configuration, MIG device exposure inside containers
- GPU-server prerequisite verification and troubleshooting
- Cross-cutting questions spanning `agent/src/tasks/` and
  `agent/src/inventory/`

## First Read

1. `docs/GPU-MIG.md`
2. `docs/ARCHITECTURE.md` § Container Labels, § GPU Slot Model

Skills: `docker-engine-api`, `nvidia-gpu-mig`.

## Invariants to Protect

- GPU/MIG assignment by NVML UUID only — indexes are display sugar.
- One deployment per slot; no oversubscription, no `--privileged`.
- Foundry never reshapes MIG geometry; it discovers operator-created
  layouts.
- Only `foundry.managed=true` containers exist for Foundry.

## Verification

Fixture tests for inventory/assignment logic; on a real GPU box the
canonical smoke test is
`docker run --rm --gpus '"device=<UUID>"' nvidia/cuda:12.4.1-base-ubuntu22.04 nvidia-smi`
showing exactly the assigned device.

## Handoff Boundaries

- Task loop / protocol semantics → `gpu-agent`
- Slot reconciliation policy on the controller → `controller`
- Driver/toolkit installation runbooks → `devops`
