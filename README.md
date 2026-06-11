# Foundry

Self-hosted GPU orchestration for GitLab-centric organizations. Deploy
Docker containers from GitLab Container Registry onto NVIDIA GPU servers —
full GPUs or MIG partitions — with an explicit drag-and-drop dashboard.
No Kubernetes. No SSH. Every action audited.

**Production:** `https://foundry.cloudcraft.ro`

## What it does

- **Login with GitLab** — one or more onboarded GitLab instances; users
  inherit their GitLab permissions automatically. Foundry keeps no
  permission system of its own.
- **Browse** projects, container registry repositories, and tags you're
  allowed to see.
- **See the fleet** — every enrolled GPU server, every GPU, every MIG
  slot, with live states (Free / Reserved / Deploying / Running / Failed /
  Stopping / Offline).
- **Drag to deploy** — drop a container image on a free slot; configure
  ports/env/volumes; replace a running workload with explicit
  confirmation.
- **Operate** — deployment lifecycle status, container logs, and a full
  append-only audit history. GPU servers need only outbound HTTPS.

## Architecture in one diagram

```
            ┌──────────────── Control plane (this host) ───────────────┐
Browser ──► │ Nginx ──► foundry-controller (Rust/axum) ──► MySQL       │
            │                    │  ▲                                  │
            │                    ▼  │ OAuth + API + Registry           │
            │              GitLab instance(s)                          │
            └────────────────────▲─────────────────────────────────────┘
                                 │ HTTPS (pull-only: heartbeat, tasks,
                                 │        inventory, logs)
            ┌────────────────────┴───── Data plane (GPU servers) ──────┐
            │ foundry-agent ──► Docker Engine + NVIDIA Toolkit + NVML  │
            └──────────────────────────────────────────────────────────┘
```

The controller never connects to GPU servers — agents pull work over
HTTPS. Details: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Tech stack

| | |
|---|---|
| Controller | Rust · axum · tokio · sqlx (MySQL) · OAuth2 |
| Agent | Rust · Docker Engine API (bollard) · NVML · systemd |
| Frontend | React · TypeScript · Vite · shadcn/ui · TanStack Query/Router · dnd-kit |
| OS | Ubuntu 24.04 everywhere |

## Repository layout

```
docs/          Knowledge base — the living spec (start: docs/ai/README.md)
docs/plans/    Per-phase plans; docs/ROADMAP.md tracks status
.claude/       AI tooling: specialist agents, skills, hooks, settings
controller/    foundry-controller binary            (Phase 1+)
agent/         foundry-agent binary                 (Phase 1+)
shared/        Wire contract: DTOs, state enums     (Phase 1+)
frontend/      React SPA                            (Phase 1+)
migrations/    sqlx MySQL migrations                (Phase 1+)
deployment/    systemd units, nginx vhost, scripts  (Phase 1+)
```

(The original spec placed `agents/` and `skills/` at the repo root; they
live under `.claude/` because that's what Claude Code loads.)

## Documentation

`docs/` is the **living source of truth** — the original bootstrap spec
was split into it and retired. Scope changes are recorded in
[docs/ROADMAP.md](docs/ROADMAP.md) § Amendments Log and the affected docs
are updated in the same commit set.

Start points: [docs/ai/README.md](docs/ai/README.md) (routing),
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md),
[docs/UI-DESIGN.md](docs/UI-DESIGN.md),
[docs/DEPLOYMENT.md](docs/DEPLOYMENT.md).

## Status

Phase 0 (documentation & AI tooling bootstrap) complete. Roadmap and
progress: [docs/ROADMAP.md](docs/ROADMAP.md).
