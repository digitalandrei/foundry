# Product Overview

Foundry is a self-hosted GPU orchestration platform for GitLab-centric
organizations. It deploys Docker containers from GitLab Container Registry
onto NVIDIA GPU servers — full GPUs, MIG partitions, or groups of GPUs —
via an explicit drag-and-drop dashboard. No Kubernetes, no SSH.

## What a user can do

- Log in with GitLab (one of the onboarded instances) and automatically
  inherit their GitLab permissions — Foundry keeps no permission system of
  its own
- Browse their GitLab projects, container registry repositories, and tags
- See all enrolled GPU servers, every GPU, every MIG slot, and each slot's
  live state (Free / Reserved / Deploying / Running / Failed / Stopping /
  Offline)
- Drag a container image onto a free slot to deploy it — ports, env, and
  persistent mounts configurable at deploy time; images that declare
  `VOLUME`s or the `ai.protv.foundry.volumes` label get their mounts
  pre-filled automatically
- Attach **persistent storage** that survives redeploys: each mount maps a
  storage source to a container bind path. Storage identity is
  `server → shared/slot/group → project (the user-given deploy name) →
  mount`; same-name redeploys reuse their data, and an Existing-root mode
  can deliberately mount any physically compatible root from prior deploys
- Browse volume files in a dual-pane manager — upload/download, copy/move,
  rename/delete, and a bounded in-browser editor — scoped to approved
  volumes only
- Replace a running workload by dropping onto an occupied slot — the
  predecessor is retained until its successor is healthy and published,
  then rolled back automatically on failure
- Open each running app at its real per-server HTTPS address, shown right
  on the slot; open an interactive shell or follow container logs live —
  operate the fleet without SSH
- Deploy one container across a **group** of whole GPUs (multi-GPU jobs),
  or soft-share a single GPU among several containers — where the operator
  has configured groups / multi-use slots

## What operators get

- Pull-only agents on GPU servers (outbound HTTPS only; no inbound
  firewall holes, no remote Docker socket)
- One-command server enrollment with single-use tokens; reusable,
  time-limited **fleet keys** auto-enroll a whole launched fleet, and
  pre-running containers can be adopted under Foundry's control
- Structured host readiness reporting (Docker, storage, NVIDIA runtime,
  nginx, TLS), on-demand diagnostics, and checksum-verified remote agent
  upgrades; GPU-runtime preflight blocks reservations on hosts that can't
  actually run GPU containers
- Fleet-wide telemetry: host, per-GPU, and per-MIG-slice memory graphs,
  storage usage with advisory quotas, and bounded per-app access logs with
  24h traffic metrics
- Append-only audit log of every state transition and admin action
- Daily verified MariaDB backups on a systemd timer; structured JSON logs
  (Prometheus `/metrics` is still pending — Phase 10)

## Key vocabulary

- **Instance** — an onboarded GitLab installation (multi-instance support)
- **Slot** — the schedulable unit: a full GPU or one MIG partition,
  identified by UUID; a multi-use slot accepts up to 4 containers
- **Group** — a named set of whole GPUs on one server; deploying to it runs
  one container across all members (multi-GPU jobs)
- **Deployment** — one container placed on a slot or group, with a full
  lifecycle state machine; its user-given name is also the storage
  namespace
- **Storage root / mount** — a named persistent directory owned by a
  placement (slot, group, or whole server) and a deploy-name project,
  bind-mounted into containers and reusable across deployments
- **Agent task** — a queued instruction an agent polls for and executes

Production URL: `https://foundry.cloudcraft.ro` (Cloudflare-proxied, Nginx
on this host). v1 scope and progress: `../ROADMAP.md`.
