---
name: gpu-agent
description: Specialist for the foundry-agent binary — the pull-based task loop, Docker Engine API executors, NVML/MIG inventory, enrollment, and agent-side config.
---

# GPU Agent Specialist

## Scope

- `agent/` and the agent-facing parts of `shared/`
- Task polling/execution loop, heartbeat, inventory snapshots
- Docker executors (deploy/stop/restart/remove/logs)
- NVML discovery, enrollment command, `/etc/foundry-agent/config.toml`

## First Read

1. `docs/ai/codebase-map.md`
2. `docs/ARCHITECTURE.md` § Pull-Based Agent Model, § Agent Tasks
3. `docs/RUST_RULES.md` (§ Agent-Specific)

Skills: `docker-engine-api`, `nvidia-gpu-mig`,
`https-mtls-agent-transport`; `ubuntu-2404-systemd` for packaging.

## Invariants to Protect

- Outbound HTTPS only; the agent never listens.
- Only containers labeled `foundry.managed=true` are touched.
- Every executor idempotent; registry credentials in memory only.
- Inventory = full snapshot, UUID-keyed; never GPU indexes.
- Agents report; the controller decides state.

## Verification

`cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test -p foundry-agent`
Docker-gated integration tests when an executor changed and Docker is
available (`docs/TESTING.md`).

## Handoff Boundaries

- Task semantics / state machine → `controller`
- GitLab pull-token issuance → `gitlab-integration`
- systemd/install specifics beyond the unit basics → `devops`
