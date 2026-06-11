# Rust Rules for Foundry

Default quality bar for `controller/`, `agent/`, and `shared/`. Adapted from
the NoSignal rule set for a CRUD/orchestration workload (no packet-frequency
hot paths here — clarity and correctness dominate).

## Core Principles

- Correctness first, then predictable behavior under failure, then
  convenience.
- Keep code minimal: no speculative hooks, no decorative abstractions, no
  layers with one caller.
- **No god files.** Small, single-responsibility modules. When a module
  accumulates a second responsibility, split it. Shared logic goes to
  `shared/` (wire types, enums, validation) — never copy-pasted between
  controller and agent.
- Reuse before writing: check `shared/` and the existing module tree for an
  existing type/function before adding a new one.
- Use the type system: enums over stringly-typed state (every state machine
  in `ARCHITECTURE.md` is a Rust enum in `shared/`), newtypes for IDs
  (`DeploymentId`, `SlotId`, ...), `Option<T>` over sentinels.

## Style

- Idiomatic Rust, rustfmt-clean. snake_case fns/modules, PascalCase types,
  SCREAMING_SNAKE_CASE consts.
- Private fields by default. Document public items that carry invariants or
  non-obvious behavior — comments explain *why* and *constraints*, not
  mechanics.
- No wildcard imports (except `use super::*;` in tests).

## Error Handling

- No `unwrap()`/`expect()` on anything fallible from network, DB, GitLab, or
  user input. `expect()` only for impossible invariants, with a message.
- `thiserror` in library code; `anyhow` only at binary boundaries.
- Propagate with `?`; add context at subsystem boundaries
  (`tracing::error!(?err, deployment_id = %id, "...")`).
- API handlers map errors to the consistent envelope in `API.md` with
  correct status codes — never leak internals or secrets in messages.

## Async (tokio)

- Never hold a `MutexGuard` across `.await`.
- Prefer message passing (`tokio::sync::mpsc`) over broad shared state;
  bound every channel and queue.
- `tokio::select!` for cancellation-safe loops (agent task loop, pollers).
- `spawn_blocking` for genuinely blocking work; never block the runtime.
- Both binaries: graceful shutdown on SIGTERM (finish in-flight task,
  flush state) — systemd restarts must be safe at any time.

## sqlx / MySQL

- Compile-checked queries (`sqlx::query!` / `query_as!`) wherever possible;
  keep `sqlx-data`/offline mode green in CI.
- Schema changes only via `migrations/` (see `mysql-schema-migrations`
  skill). Never `ALTER` outside a migration.
- Transactions around every multi-row state change (deployment transition +
  event + audit row commit together, or not at all).
- All timestamps UTC; UUIDs as BINARY(16) with newtype wrappers.

## State Machines

- Slot and deployment transitions go through a single transition function
  per machine that validates legality, persists the new state, writes the
  `deployment_events` row, and the audit row — in one transaction. No
  scattered `UPDATE ... SET state` calls.

## Agent-Specific

- Task execution idempotent (re-delivery after crash is normal).
- Only ever touch containers labeled `foundry.managed=true`.
- Registry credentials live in memory for the duration of a pull, never on
  disk, never in logs.
- Inventory uploads are full snapshots; do not maintain incremental diff
  state in the agent.

## Logging & Security

- `tracing` only (structured JSON in production); no `println!`/`dbg!`.
- Log state changes and decisions, not loops. Never log tokens, secrets,
  cookies, or registry credentials.
- Validate all external input at the boundary; agents are authenticated but
  still bounds-checked.

## Dependencies

- Keep them minimal and justified; prefer std/existing deps. The core set:
  axum, tokio, sqlx, reqwest (rustls), serde, tracing, thiserror, uuid,
  chrono, oauth2, bollard (agent), nvml-wrapper (agent). Adding beyond this
  requires a stated reason in the PR/commit.

## Testing & Tooling

- New non-trivial behavior ships with tests; bug fixes tighten a test.
  See `TESTING.md`.
- Before claiming completion on touched crates:
  `cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test`.
