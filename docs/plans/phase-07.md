# Phase 7 — Logs

**Status:** Not started · refine this plan right before starting.

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
