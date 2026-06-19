# Plan 006: Add a CI pipeline that enforces `scripts/check.sh` on push/PR

> **Executor instructions**: Follow this plan step by step. Run every
> verification command. If a STOP condition occurs, stop and report. When done,
> update this plan's row in `advisor-plans/README.md`.
>
> **⚠ This plan reverses a recorded decision.** Do not start it until the
> operator has explicitly approved adding CI (see "Why this matters"). If you
> were dispatched without that approval, STOP and ask.
>
> **Drift check (run first)**: `git diff --stat 14f9d95..HEAD -- scripts/check.sh docs/ROADMAP.md` and `ls .github/workflows`.

## Status

- **Priority**: P2 (gated on operator approval)
- **Effort**: S–M
- **Risk**: LOW (no production-path code changes)
- **Depends on**: 003 (so CI runs frontend tests), 004 (so CI can run
  `typecheck`) — CI can land first with existing gates and pick those up after.
- **Category**: dx
- **Planned at**: commit `14f9d95`, 2026-06-19

## Why this matters

The verification gate (`scripts/check.sh`: fmt + clippy `-D warnings` + test,
then frontend lint + build) runs **only on the developer's machine, by
convention**. Nothing enforces it: a commit with a clippy warning, a failing
test, or a broken frontend build can land on `main` unnoticed. As the project
moves toward production (Phase 10) and as plans here get executed by *other
agents*, an enforced green gate is what lets you trust a handoff without
re-running everything by hand — which is precisely the workflow the architect
agent is meant to support.

**Doctrine tension — read before proceeding.** `docs/ROADMAP.md` records a
deliberate decision (2026-06-11, confirmed Phase 2):

> **No CI.** Deploying is easy enough from this host; `scripts/check.sh` is the
> verification gate, run locally before every commit.

This plan reverses that. That is a maintainer call, not an executor's — the
plan therefore **amends the ROADMAP decision as its first step** and must not
run without explicit operator approval. The repo has a GitHub remote
(`github.com:digitalandrei/foundry`), so GitHub Actions is the mechanism.

## Current state

- No `.github/workflows/`, no `.gitlab-ci.yml`, no `Makefile`.
- `scripts/check.sh` is the single source of truth for "is it green":
  ```bash
  cargo fmt --all -- --check
  cargo clippy --all-targets -- -D warnings
  cargo test
  if [ -f frontend/package.json ]; then
    (cd frontend && npm run lint && npm run build)
  fi
  ```
- `cargo test` and sqlx compile-time checks need a MySQL `DATABASE_URL`. CI must
  provide a MySQL service + run the embedded migrations, OR the sqlx offline
  cache (`.sqlx/`) must be committed so `cargo check` works without a DB.
  Determine which by checking for a committed `.sqlx/` dir; if absent, the CI
  job needs a MySQL service container.

## Commands you will need

| Purpose | Command | Expected |
|---------|---------|----------|
| Local gate (mirror of CI) | `bash scripts/check.sh` | "check.sh: all gates passed" |
| Lint the workflow (optional) | `actionlint .github/workflows/ci.yml` | exit 0 (if `actionlint` available) |
| Check for sqlx offline cache | `ls .sqlx 2>/dev/null` | present → no DB service needed |

## Scope

**In scope**:
- `.github/workflows/ci.yml` (new)
- `docs/ROADMAP.md` (amend the "No CI" decision)
- Possibly committing the sqlx offline cache (`.sqlx/`) IF that's the chosen way
  to avoid a CI database — decide in Step 2

**Out of scope**:
- Any application source change. CI must pass against the code as-is; if
  `check.sh` currently fails on `main`, STOP and report (fixing it is a
  separate finding, not this plan).
- Deployment automation (CD) — this is build/test gating only. `scripts/deploy.sh`
  stays the manual, operator-run path.
- Branch-protection settings (a GitHub UI/admin action the operator owns).

## Git workflow

- Branch: `advisor/006-ci-pipeline`
- Commits: one for the workflow, one for the ROADMAP amendment.
- Do NOT push unless instructed (the operator may want to land this themselves
  given it's a policy change).

## Steps

### Step 1: Amend the ROADMAP decision

In `docs/ROADMAP.md` § Amendments Log, update the "No CI" entry to record the
reversal with today's date and the rationale (enforced gate for agent-executed
work + Phase 10 readiness). Keep the history — note it supersedes the
2026-06-11 decision rather than deleting that line.

**Verify**: `grep -n -i 'CI' docs/ROADMAP.md` shows the superseding entry.

### Step 2: Decide the database strategy for `cargo test`

- If `.sqlx/` offline cache exists and is committed → CI can run
  `cargo check`/`clippy` with `SQLX_OFFLINE=true` and no DB. `cargo test` still
  needs a DB if any test hits MySQL; gate those behind a MySQL service.
- If no offline cache → add a `services: mysql:8` block to the job, set
  `DATABASE_URL` to it, and let the controller's embedded migrations run (they
  run at startup; the test harness applies them).

Pick one, and write it down in the workflow comments so it's not re-litigated.

### Step 3: Write `.github/workflows/ci.yml`

A single workflow, triggered on `push` and `pull_request`, that reproduces
`scripts/check.sh`:

- Job `rust`: checkout → install stable toolchain with `rustfmt` + `clippy` →
  cache cargo → (MySQL service or `SQLX_OFFLINE=true` per Step 2) →
  `cargo fmt --all -- --check` → `cargo clippy --all-targets -- -D warnings` →
  `cargo test`.
- Job `frontend`: checkout → setup Node (match the version the repo uses) →
  `cd frontend && npm ci` → `npm run lint` → (`npm run typecheck` if plan 004
  landed) → (`npm run test:run` if plan 003 landed) → `npm run build`.

Prefer invoking `bash scripts/check.sh` directly where feasible so CI and local
stay identical — but split into two jobs if it gives useful parallelism and
clearer logs. Keep it boring and inspectable (doctrine: explicit over clever).

**Verify**: if `actionlint` is available, `actionlint .github/workflows/ci.yml`
→ exit 0. Otherwise eyeball the YAML for valid structure.

### Step 4: Confirm the gate reflects reality locally

**Verify**: `bash scripts/check.sh` → "check.sh: all gates passed" on the
current tree (proves CI will be green on merge, not red on arrival).

## Done criteria

- [ ] `.github/workflows/ci.yml` exists and runs fmt + clippy(-D warnings) +
      test for Rust, and lint + build for frontend
- [ ] The workflow's DB strategy (Step 2) is implemented and commented
- [ ] `docs/ROADMAP.md` records the reversal of the "No CI" decision
- [ ] `bash scripts/check.sh` passes locally
- [ ] `advisor-plans/README.md` status row updated

## STOP conditions

- The operator has not approved adding CI — STOP (this is a policy reversal).
- `bash scripts/check.sh` does not currently pass on `main` — STOP and report
  the failing gate; CI shouldn't be introduced red.
- The DB strategy is ambiguous (no offline cache AND tests need a live schema
  you can't stand up in CI) — STOP and ask which path the operator prefers.

## Maintenance notes

- Once green, the operator can enable branch protection (require the `rust` and
  `frontend` checks) — that's a repo-settings action outside this plan.
- Keep CI a literal mirror of `scripts/check.sh`. If the local gate gains a
  step, CI gains the same step in the same commit, or they drift and CI lies.
- As plans 003/004 land, wire `npm run test:run` / `npm run typecheck` into the
  frontend job.
