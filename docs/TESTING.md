# Testing Strategy

Per-layer conventions. New non-trivial behavior ships with tests; bug fixes
add or tighten one. Don't claim completion without running the relevant set.

## Controller (`controller/`, `shared/`)

- **Unit tests** for pure logic: state-machine transition legality (every
  slot/deployment transition table case), validation in `shared/`, GitLab
  response parsing (recorded JSON fixtures, no live GitLab in tests).
- **Integration tests** against a real MySQL (`sqlx::test` with a dedicated
  test database; migrations applied automatically). Cover: enrollment flow,
  task queue dispatch/result handling, deployment transaction atomicity
  (state + event + audit commit together).
  *Open item (noted Phase 3):* the scoped `foundry` DB user cannot
  create `sqlx::test` databases; the harness needs either a dedicated
  `foundry_test` DB + grant or a test-only MySQL user. Until then,
  route behavior is verified live (curl probes: auth required on every
  protected route, error envelopes, health) as done in Phase 3.
- HTTP-level tests with `axum`'s `tower::ServiceExt::oneshot` — auth
  required on every `/api` and `/agent` route is itself a test.
- sqlx note: `query!` macros compile against the live dev DB
  (`DATABASE_URL` from `.env`); building on a host without the DB
  requires `cargo sqlx prepare` offline data (not set up — single-host
  project, no CI by decision).

## Agent (`agent/`)

- Docker interactions behind a thin trait so the task executors are testable
  with a mock; integration tests against a real local Docker daemon are
  gated behind an env flag (CI/dev hosts with Docker only).
- NVML discovery behind the same pattern — fixture-based inventory tests
  (A100 MIG layouts, non-MIG GPUs, geometry changes).
- Idempotency tests: every task type executed twice yields the same end
  state.

## Frontend (`frontend/`)

- `npm run build` (TypeScript strict) is the minimum gate for every change.
- Component tests (Vitest + Testing Library) for load-bearing pieces: slot
  chip state rendering, state→color map, replacement confirmation dialog,
  deployment form zod schemas.
- Both themes spot-checked for new screens (manual until visual testing is
  added).

## Commands

```bash
cargo test                       # workspace
cargo test -p foundry-controller
cargo test -p foundry-agent
cargo clippy --all-targets -- -D warnings
cd frontend && npm run build && npm test
```
