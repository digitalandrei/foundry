# Product Overview

Foundry is a self-hosted GPU orchestration platform for GitLab-centric
organizations. It deploys Docker containers from GitLab Container Registry
onto NVIDIA GPU servers — full GPUs or MIG partitions — via an explicit
drag-and-drop dashboard. No Kubernetes, no SSH.

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

## What operators get

- Pull-only agents on GPU servers (outbound HTTPS only; no inbound
  firewall holes, no remote Docker socket)
- One-command server enrollment with single-use tokens, rotatable agent
  credentials
- Append-only audit log of every state transition and admin action
- Prometheus metrics, structured JSON logs

## Key vocabulary

- **Instance** — an onboarded GitLab installation (multi-instance support)
- **Slot** — the schedulable unit: a full GPU or one MIG partition,
  identified by UUID
- **Deployment** — one container placed on one slot, with a full lifecycle
  state machine
- **Agent task** — a queued instruction an agent polls for and executes

Production URL: `https://foundry.cloudcraft.ro` (Cloudflare-proxied, Nginx
on this host). v1 scope and progress: `../ROADMAP.md`.
