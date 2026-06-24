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
- Drag a container image onto a free slot to deploy it (ports, env,
  volumes configurable at deploy time)
- Replace a running workload by dropping onto an occupied slot — with an
  explicit confirmation step
- Watch deployment status move through its lifecycle, view container logs,
  and review the full audit history of every action
- Open an interactive shell on a running container straight from the
  browser, or follow its logs live — operate the fleet without SSH
- Deploy one container across a **group** of whole GPUs (multi-GPU jobs),
  or soft-share a single GPU among several containers — where the operator
  has configured groups / multi-use slots

## What operators get

- Pull-only agents on GPU servers (outbound HTTPS only; no inbound
  firewall holes, no remote Docker socket)
- One-command server enrollment with single-use tokens; reusable,
  time-limited **fleet keys** auto-enroll a whole launched fleet, and
  pre-running containers can be adopted under Foundry's control
- Fleet-wide telemetry: host, per-GPU, and per-MIG-slice memory graphs
  across every enrolled server
- Append-only audit log of every state transition and admin action
- Prometheus metrics, structured JSON logs

## Key vocabulary

- **Instance** — an onboarded GitLab installation (multi-instance support)
- **Slot** — the schedulable unit: a full GPU or one MIG partition,
  identified by UUID; a multi-use slot accepts up to 4 containers
- **Group** — a named set of whole GPUs on one server; deploying to it runs
  one container across all members (multi-GPU jobs)
- **Deployment** — one container placed on a slot or group, with a full
  lifecycle state machine
- **Agent task** — a queued instruction an agent polls for and executes

Production URL: `https://foundry.cloudcraft.ro` (Cloudflare-proxied, Nginx
on this host). v1 scope and progress: `../ROADMAP.md`.
