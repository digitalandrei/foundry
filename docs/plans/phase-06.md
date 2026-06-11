# Phase 6 — Deployments (Lifecycle & Replacement)

**Status:** Not started · refine this plan right before starting.

## Goal

The core feature: deploy a registry image to a slot, full lifecycle, and the
replacement workflow. Implements `../ARCHITECTURE.md` § Deployment Lifecycle
and § Agent Tasks.

## Deliverables

- Task queue: `agent_tasks`/`agent_task_results`, `GET /agent/tasks/next`
  (long-poll), `POST /agent/tasks/result`, re-dispatch on timeout
- Deployment state machine in `shared` + single transition function
  (state + event + audit in one transaction, `../RUST_RULES.md` § State
  Machines)
- `POST /api/deployments` (validation: slot FREE, user may pull the tag),
  stop/restart/delete endpoints
- Replacement: `POST /api/deployments/{id}/replace` —
  stop old → remove old → pull new → start new; old ends `REPLACED`
- Agent executors: DEPLOY/STOP/RESTART/REMOVE_CONTAINER via bollard —
  labels, GPU device requests by UUID, ports/env/volumes, short-lived pull
  credentials (`../GITLAB-INTEGRATION.md` § Image Pulls)
- UI: dnd-kit drag from sidebar card to slot chip, deployment config dialog
  (RHF+zod), replacement confirmation dialog, Running Deployments table
  with live state

## Acceptance

- Drag-deploy of a real image onto a real MIG slot reaches RUNNING with GPU
  visible inside the container; replace works with confirmation; every
  transition has a `deployment_events` + audit row; idempotency tests pass
