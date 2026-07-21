# Testing Strategy

Per-layer conventions. New non-trivial behavior ships with tests; bug fixes
add or tighten one. Don't claim completion without running the relevant set.

## Controller (`controller/`, `shared/`)

- **Unit tests** for pure logic: state-machine transition legality (every
  slot/deployment transition table case), validation in `shared/`, GitLab
  response parsing (recorded JSON fixtures, no live GitLab in tests).
  Registry-config coverage includes compressed manifest-size parsing,
  Foundry-label precedence over standard Docker volume declarations, and
  rejection of unsafe declared mount defaults. 0.59.0 adds app-label policy,
  digest pull-scope, strict readiness evidence, agent-version and request
  percentile regression coverage.
- **Integration tests** against a real MySQL (`sqlx::test` with a dedicated
  test database; migrations applied automatically). Cover: enrollment flow,
  task queue dispatch/result handling, deployment transaction atomicity
  (state + event + audit commit together).
  The MariaDB GitHub Actions job supplies a privileged disposable test-only
  user/database; tests are `#[ignore]` in the fast offline suite and CI runs
  them explicitly with `cargo test -p foundry-controller -- --ignored`.
  Covered today: migrated health router; atomic deployment reservation/task/
  event/audit success; authoritative external-GPU zero-write rejection;
  repository + database adoption uniqueness; enrollment token/credential/audit
  consistency; concurrent GitLab project/repository/tag mirror upserts;
  project-shared reuse vs creator-private volume isolation; batched fleet
  output shape; active deployment-name uniqueness per server; and
  allowed/blocked guarded server removal.
- HTTP-level tests with `axum`'s `tower::ServiceExt::oneshot` — auth
  required on every `/api` and `/agent` route is itself a test.
- sqlx note: `query!` macros compile against the live dev DB
  (`DATABASE_URL` from `.env`) locally, or the committed offline cache
  (`.sqlx/`, `SQLX_OFFLINE=true`) on a host without the DB — which is how
  CI builds. Regenerate the cache with `cargo sqlx prepare --workspace`
  whenever a query changes (a stale cache fails the build).

## Agent (`agent/`)

- Docker interactions behind a thin trait so the task executors are testable
  with a mock; integration tests against a real local Docker daemon are
  gated behind an env flag (CI/dev hosts with Docker only).
- NVML discovery behind the same pattern — fixture-based inventory tests
  (A100 MIG layouts, non-MIG GPUs, geometry changes).
- Idempotency tests: every task type executed twice yields the same end
  state.
- Executor regressions cover digest-only preflight, prepare-without-create,
  retained quiesce/rollback, successful no-HEALTHCHECK startup, normal stop,
  pull/auth failures and create conflicts. Host rendering tests cover nginx
  JSON log policy, logrotate and capability bounds; health/publication failure
  branches remain behind the same fake-Docker/vhost seams.
- Persistent-directory executors hard-reject every remove/purge target
  outside `/storage/containers/`; purge batches run before deploy as one
  sequential task. Controller version parsing prevents PURGE_VOLUMES dispatch
  to pre-0.54 agents.
- Volume-file path tests reject absolute/traversal paths, the storage root
  itself, and symlink components. Mutation-audit coverage verifies editor and
  transfer content is never copied into the audit detail.
- Resumable upload tests assert stable partial-file identity; traffic tests
  cover nginx JSON parsing and cursor advancement only after controller ack so
  transient failures do not drop request records.

## Frontend (`frontend/`)

- `npm run build` (TypeScript strict) and `npm run lint` are gates for every
  change; `npm run typecheck` (`tsc --noEmit`) is the fast iteration check.
- **Vitest harness** (`npm run test:run`, jsdom env). Covered today: the pure
  `lib/` logic — the state→color map (`states.test.ts`) and slot
  occupancy/deploy-eligibility (`slots.test.ts`), plus persistent-mount policy
  validation/ComfyUI 8188 classification (`deployment-form.test.ts`) and
  volume path/version helpers (`volume-files.test.ts`). Add tests beside the module,
  importing `{ describe, it, expect }` from `vitest`.
  Primary app URL selection and 0.59 agent feature gates are also covered.
- **Testing Library component/DOM coverage** now includes named/focusable
  GPU interaction surfaces and Enter/Space activation under both theme
  classes, plus keyboard opening and accessible naming for both themes of the
  volume file pane. Open items remain for load-bearing pieces — deploy-dialog zod
  validation, the type-to-confirm gate on destructive ops for adopted
  containers, slot-chip rendering — and visual/both-theme regression. Both
  themes are spot-checked manually until then.

## Commands

```bash
SQLX_OFFLINE=true bash scripts/check.sh  # canonical local gate

# Disposable privileged MariaDB only (CI runs this automatically):
DATABASE_URL=mysql://... SQLX_OFFLINE=true \
  cargo test -p foundry-controller -- --ignored
```

The canonical gate includes fmt, clippy, Rust tests, `cargo deny`, the
hermetic backup test, npm audit, frontend lint/DOM tests, and production build.
CI additionally performs a real MariaDB dump/restore round trip.
