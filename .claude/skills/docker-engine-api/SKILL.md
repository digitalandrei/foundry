---
name: docker-engine-api
description: >
  For the Foundry project at /opt/foundry. Docker Engine API usage in the
  foundry-agent (bollard): authenticated pulls, container create with
  Foundry labels and GPU device requests, lifecycle operations, and the
  only-touch-managed-containers rule. Use when implementing or debugging
  agent task executors.
---

# Docker Engine API (Agent)

The agent talks to the **local** Docker socket only (`bollard` with unix
socket). The Docker API is never exposed remotely
(`docs/SECURITY.md`).

## The Prime Directive

Foundry manages **only** containers labeled `foundry.managed=true`. Every
list/stop/remove operation filters on that label. Containers without it do
not exist as far as Foundry is concerned — never enumerate them into
inventory, never touch them.

## Labels on Create

```
foundry.managed=true
foundry.deployment_id=<uuid>
foundry.slot_id=<uuid>
```

These are the join keys between Docker reality and controller state; set
all three on every create, and use them to find containers (not names).

## Pull

- `create_image` with `X-Registry-Auth` built from the short-lived
  credential in the task payload (`gitlab-api-oauth-registry` skill).
- Credential lives in memory for the pull only — not in config, not on
  disk, not in logs (mask in error contexts too).
- Pull by tag + verify digest from the task payload when present.

## Create with GPU

`HostConfig.DeviceRequests` with the slot's UUID:

```json
{ "Driver": "nvidia", "DeviceIDs": ["MIG-xxxx..."], 
  "Capabilities": [["gpu"]] }
```

- `DeviceIDs` takes the NVML **UUID** (GPU-... for FULL_GPU slots,
  MIG-... for MIG slots) — never an index (`docs/GPU-MIG.md`).
- Ports/env/volumes exactly as the deployment declares; no `--privileged`,
  no extra capabilities (v1).
- Restart policy: none — lifecycle is controller-driven; the agent reports
  exits and the controller decides.

## Lifecycle Ops & Idempotency

Every executor must be safe to run twice (task re-delivery is normal):

- DEPLOY: if a container with this `deployment_id` label already exists in
  the desired state, report success.
- STOP/REMOVE: "already stopped"/"not found" (404) on a managed container
  counts as success.
- Bounded timeouts on stop (grace period then kill), pull (overall
  deadline), and log reads (size caps per `docs/plans/phase-07.md`).

## Failure Reporting

Executor errors map to a structured task result (`agent_task_results`) —
include the Docker error class (not raw internals) so the controller can
set `FAILED` with a usable `error_message`.
