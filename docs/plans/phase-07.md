# Phase 7 — Logs

**Status:** ✅ Done — agent push-loop log capture + bounded 7-day storage +
UI viewer + destructive-action confirmation are live end-to-end; GPU-server
agents capture on enrolled servers (running recent builds). Implementation
diverged from the original deliverables in two deliberate ways — recorded
below.

## Decisions taken (vs the original sketch)

- **Push loop, not `UPLOAD_LOGS` task.** The agent ships incremental
  log chunks every 10s (same architecture as `/agent/metrics`) rather
  than the controller enqueuing a task — the sequential task loop would
  block deploys, and a push loop keeps the viewer continuously fresh.
  The `UPLOAD_LOGS` task type is retained but unused.
- **Poll-tail, not SSE** (the plan's decision point). The UI polls
  `GET /api/deployments/{id}/logs` every 3s while Follow is on; every
  other view already polls and a 3s tail is live enough. Recorded in
  API.md § Logs design and DEPLOYMENT.md.
- **Incremental capture** via a per-deployment `docker logs --since`
  cursor (+ sub-second dedup), so only new output ships.
- **Retention bounded twice** (operator: "keep only the last 7 days at
  most"): a half-hourly sweeper drops chunks older than 7 days, and each
  append trims the deployment to its newest N chunks so a log-spamming
  container is capped within one interval. Logs are deleted with the
  deployment when it goes REMOVED (the `transition_deployment` choke
  point); a STOPPED deployment keeps its logs.
- **Only managed containers** are read (label `foundry.managed=true`);
  foreign containers stay detect-only (slot locked, name+telemetry).

## Goal

Operators can read container logs for any deployment from the UI, without
SSH.

## Deliverables

- Agent: `UPLOAD_LOGS` task executor — collect container logs (bounded
  chunks) and upload via `POST /agent/logs`; periodic tail upload for
  RUNNING deployments
- Controller: log storage strategy (retention-bounded), 
  `GET /api/deployments/{id}/logs` with pagination/tail semantics
- UI: log viewer on deployment detail (monospace, follow mode, copy);
  console action button in the deployments table wired up
- Decision point: live streaming (SSE through the existing Nginx
  upgrade-ready vhost) vs poll-tail for v1 — record in `../API.md` and
  `../DEPLOYMENT.md`

## Acceptance

- Logs of a running and of a stopped deployment readable in the UI; size
  bounds enforced (a log-spamming container cannot exhaust the controller)
