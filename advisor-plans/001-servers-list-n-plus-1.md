# Plan 001: Eliminate the per-server N+1 in the servers list endpoint

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `advisor-plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 14f9d95..HEAD -- controller/src/repos/servers.rs controller/src/repos/inventory.rs`
> If either in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: none
- **Category**: perf
- **Planned at**: commit `14f9d95`, 2026-06-19

## Why this matters

The fleet dashboard polls `GET /api/servers` every 10s
(`frontend/src/hooks/use-servers.ts`). The handler builds each
`ServerSummary` by issuing two extra round-trips **per server** inside a loop,
and `gpus_for_server` is itself a multi-query assembly. Worse,
`get_summary(id)` — the single-server endpoint used by `/api/servers/{id}` —
calls `list()` and throws away every row but one, so fetching *one* server
loads the entire fleet's GPU trees. At 10 servers and a 10s refetch this is
~hundreds of avoidable queries per minute and it scales linearly with fleet
size, exactly as the operator adds GPU hosts.

This plan removes the cheap, low-risk portion of the N+1 (the per-server
`running_count`, and the `get_summary`→`list` blowup). Batching the heavier
`gpus_for_server` assembly is explicitly deferred — see Maintenance notes.

## Current state

- `controller/src/repos/servers.rs` — `list()` and `get_summary()`.

`list()` issues two awaits per row inside the loop (`servers.rs:33-50`):

```rust
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let status: ServerStatus = r.status.parse().map_err(AppError::internal)?;
        let id: ServerId = r.id.into();
        out.push(ServerSummary {
            // ...fields from r...
            enrolled: r.agent_id.is_some(),
            gpus: super::inventory::gpus_for_server(pool, id).await?,          // N+1
            containers_running: super::inventory::running_count(pool, id).await?, // N+1
        });
    }
```

`get_summary()` re-runs the whole fleet query to return one row
(`servers.rs:70-76`):

```rust
pub async fn get_summary(pool: &MySqlPool, id: ServerId) -> Result<ServerSummary, AppError> {
    list(pool)
        .await?
        .into_iter()
        .find(|s| s.id == id)
        .ok_or(AppError::NotFound("server not found"))
}
```

`running_count` is a single COUNT that can be folded into the main query
(`inventory.rs:500-507`):

```rust
pub async fn running_count(pool: &MySqlPool, server_id: ServerId) -> Result<i64, AppError> {
    Ok(sqlx::query_scalar!(
        "SELECT COUNT(*) FROM server_containers WHERE server_id = ? AND state = 'running'",
        server_id.0
    ).fetch_one(pool).await?)
}
```

`gpus_for_server` (`inventory.rs:353+`) is a richer nested assembly
(external occupants + group memberships + gpus + per-gpu slots). **Do not
inline it into the JOIN** — it is the deferred part.

Repo conventions to match:
- sqlx macros with explicit type overrides (`AS "id: Uuid"`, `AS "x: bool"`),
  named queries, errors via `AppError` and `.map_err(AppError::internal)` for
  parse failures. See `servers.rs:18-53` as the exemplar for the query+map
  shape.
- One responsibility per repo fn; this stays in `servers.rs`.

## Commands you will need

| Purpose   | Command                                              | Expected on success |
|-----------|------------------------------------------------------|---------------------|
| Build     | `cargo build -p foundry-controller`                  | exit 0              |
| Typecheck queries | `cargo check -p foundry-controller`          | exit 0 (sqlx compiles the SQL) |
| Lint      | `cargo clippy -p foundry-controller --all-targets -- -D warnings` | exit 0 |
| Tests     | `cargo test -p foundry-controller`                   | all pass            |
| Full gate | `bash scripts/check.sh`                              | "check.sh: all gates passed" |

> sqlx here uses compile-time-checked macros against a live DB via
> `DATABASE_URL` (offline cache may be absent). If `cargo check` fails with
> "set DATABASE_URL" or "no cached data", that is environment setup, **not**
> your change — see STOP conditions.

## Scope

**In scope** (the only files you should modify):
- `controller/src/repos/servers.rs`

**Out of scope** (do NOT touch):
- `controller/src/repos/inventory.rs` — `gpus_for_server` stays as-is this
  plan. Folding it in is a separate, riskier change (Maintenance notes).
- Any `ServerSummary` field shape (`shared/src/dto/`) — the API response must
  not change; this is a query-shape optimization only.
- `frontend/` — the response is identical, so the UI needs no change.

## Git workflow

- Branch: `advisor/001-servers-n-plus-1`
- Commit per logical step; message style matches `git log` (terse, lower-case
  imperative subject, e.g. `servers: fold running_count into list query`).
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Give `get_summary` a direct single-server path

Replace the `list()`-then-`find()` body so a single-server fetch no longer
loads the fleet. Reuse the existing per-server helpers for the one server:
build the header row with a `WHERE s.id = ?` variant of the `list()` SELECT,
then call `gpus_for_server(pool, id)` and `running_count(pool, id)` for that
one id only.

Keep the SELECT column list and the `LEFT JOIN server_agents` identical to
`list()` so the two stay in sync; only the `WHERE`/`ORDER BY` differ. Map the
row into `ServerSummary` with the same field mapping used in `list()`.

**Verify**: `cargo check -p foundry-controller` → exit 0.

### Step 2: Fold `running_count` into the `list()` main query

Add a correlated/grouped count of running `server_containers` to the `list()`
SELECT so the per-row `running_count(pool, id).await?` call is removed.
Prefer a `LEFT JOIN (SELECT server_id, COUNT(*) AS running FROM
server_containers WHERE state = 'running' GROUP BY server_id) c ON
c.server_id = s.id` and read `COALESCE(c.running, 0) AS "containers_running: i64"`.
Delete the `containers_running: super::inventory::running_count(...)` call
from the loop and read the column instead.

Apply the **same** change to the Step 1 single-server query so both paths
share the shape.

**Verify**: `cargo clippy -p foundry-controller --all-targets -- -D warnings`
→ exit 0, and `cargo test -p foundry-controller` → all pass.

### Step 3: Confirm behavior is unchanged

`gpus_for_server` is still called per server in `list()` — that is expected
and deferred. Confirm the only removed call is `running_count` and that
`get_summary` no longer calls `list()`.

**Verify**: `grep -n 'running_count\|list(pool)' controller/src/repos/servers.rs`
→ no `running_count` call remains in `servers.rs`; `list(pool)` appears only
as the `pub async fn list` definition, not inside `get_summary`.

## Test plan

- Add a `#[sqlx::test]` (or the repo's existing controller test harness — check
  `controller/src/repos/local_admins.rs` for the established test style) that:
  1. inserts a server with two `server_containers` (one `running`, one
     `exited`) and asserts `get_summary().containers_running == 1`;
  2. inserts two servers and asserts `list()` returns both with correct
     per-server `containers_running` (proves the GROUP BY keyed correctly and
     a server with zero running containers reports 0, not NULL).
- If the controller has no DB-backed test harness wired yet (integration tests
  are a known open item in `docs/TESTING.md`), STOP and report rather than
  inventing one — note that the change is covered by `cargo check`'s
  compile-time SQL verification only.
- Verification: `cargo test -p foundry-controller` → all pass, new tests run.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `cargo check -p foundry-controller` exits 0
- [ ] `cargo clippy -p foundry-controller --all-targets -- -D warnings` exits 0
- [ ] `cargo test -p foundry-controller` exits 0
- [ ] `grep -n "running_count" controller/src/repos/servers.rs` returns nothing
- [ ] `get_summary` no longer contains `list(pool)`
- [ ] `git diff --stat` shows only `controller/src/repos/servers.rs` changed
- [ ] `advisor-plans/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- The "Current state" excerpts don't match live code (drift since `14f9d95`).
- `cargo check` fails only with a `DATABASE_URL` / sqlx-offline error — this is
  environment setup; report it, do not try to change SQL to make it pass.
- Folding `running_count` requires touching `ServerSummary`'s type — it must
  not; if the column type won't map to the existing `containers_running`
  field, stop.
- You find a caller of `get_summary` that depended on the side effect of
  loading the whole fleet (there should be none).

## Maintenance notes

- **Deferred follow-up (separate plan)**: batch `gpus_for_server` across all
  servers in one pass. It assembles external occupants + group memberships +
  gpus + per-gpu slots; doing it set-at-a-time is an L-effort, MED-risk change
  that needs its own characterization tests. Measure first: log query counts
  for `GET /api/servers` before deciding it's worth it.
- A reviewer should confirm the GROUP BY count handles servers with **zero**
  running containers (COALESCE to 0) and that the single-server query column
  list stays identical to `list()` — drift between the two is the likely
  future bug.
