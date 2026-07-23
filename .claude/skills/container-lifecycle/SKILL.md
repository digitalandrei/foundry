---
name: container-lifecycle
description: >
  For the Foundry project at /opt/foundry. The deployment lifecycle state
  machine, slot states, replacement workflow, failure/retry semantics, and
  audit events. Use when implementing or reviewing any code that moves a
  deployment or slot between states.
---

# Container Lifecycle

State machines are defined in `docs/ARCHITECTURE.md` (§ Slot States,
§ Deployment Lifecycle, § Replacement workflow) and encoded as enums in
`shared/`. This skill is the implementation discipline.

## The One Transition Function Rule

Each state machine has exactly one transition function
(`controller/src/lifecycle/`). It:

1. validates the transition is legal (table-driven; illegal → typed error),
2. updates the state row,
3. inserts the `deployment_events` row (from, to, actor, detail),
4. inserts the `audit_logs` row,

— all in **one MySQL transaction**. No scattered `UPDATE ... SET state`
anywhere else in the codebase; reviewers treat one as a bug.

## Deployment Flow (happy path)

`PENDING` (created via API, slot atomically moved FREE→RESERVED)
→ `VALIDATING` (user may pull tag? slot still healthy?)
→ task enqueued → agent: `PULLING_IMAGE` → `CREATING_CONTAINER`
→ `STARTING` → `RUNNING` (slot → RUNNING).
Stop: `STOPPING` → `STOPPED` (slot stays `RESERVED` — the spec keeps its
place). Then `RESTARTING` → `RUNNING`, or `REMOVING` → `REMOVED`.

Agent task results drive the agent-side transitions; the controller maps
each result onto the machine — agents never write state directly.

**Teardown leaves no host garbage.** `STOP` and `REMOVE` both delete the
container and then reclaim its image best-effort (an image still used by a
sibling deployment is left alone). A STOPPED deployment thus has no
container to start, so **restart re-deploys**: the restart route calls
`enqueue_restart`, which transitions `STOPPED → RESTARTING` and enqueues
`DEPLOY_CONTAINER`; the deploy result then drives `RESTARTING → RUNNING`.
The agent's `RESTART_CONTAINER` executor still exists but the restart
action no longer uses it.

## Failure Semantics

- Any step failing → `FAILED` with `error_message`; the slot goes `FAILED`
  until cleanup (remove container remnants) returns it to `FREE`.
- No automatic retry in v1 — failures are explicit and user-retriable
  (operational clarity over magic). Re-dispatch of an *unacknowledged*
  task after agent crash is not a retry; executors are idempotent.
- Re-dispatch is bounded (5 attempts, 0.66.0): an exhausted task is
  **abandoned** by a controller sweeper — terminal `FAILED` task, a
  synthetic failure result, and the deployment driven through the normal
  failure mapping with actor `CONTROLLER` plus a `TASK_ABANDONED` audit
  record (`repos/tasks.rs::abandon_exhausted`).
- Slot vanishing (MIG geometry change / server offline) while RUNNING:
  slot → `OFFLINE`, deployment flagged; resolution is operator-driven.

## Replacement

Drop on occupied slot → confirmation (UI shows current deployment) →
on confirm: stop old → remove old → pull new → start new, as one ordered
task sequence. Old deployment ends `REPLACED` with
`replaced_by_deployment_id` linking to the new one; both directions
auditable. Cancel at confirmation leaves everything untouched.

## Audit Invariants

- `deployment_events` and `audit_logs` are append-only.
- Every transition carries an actor (`USER`/`AGENT`/`CONTROLLER`) — UI
  timelines and the audit page are reconstructions of these rows, nothing
  else.
