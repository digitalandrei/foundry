---
name: rust-axum-sqlx
description: >
  For the Foundry project at /opt/foundry (Rust workspace, MySQL via sqlx).
  Backend patterns for the foundry-controller: axum routing, tokio async
  discipline, sqlx/MySQL queries and transactions, error envelopes, tracing.
  Use when designing or implementing controller APIs, business logic, the
  task queue, or reviewing backend code.
---

# Rust + axum + sqlx Backend

Production patterns for `controller/` and `shared/`. Full rule set:
`docs/RUST_RULES.md` — this skill is the working subset.

## Stack

Rust stable, tokio (multi-thread), axum, sqlx (MySQL), reqwest (rustls),
serde, tracing, thiserror, oauth2.

## Routing & Handlers

- Group routes by resource under `controller/src/routes/`; one module per
  resource, no god routers.
- Extractors do the validation: `Json<CreateDeployment>` where the DTO (in
  `shared/`) carries serde+validation; `Path<DeploymentId>` with ID
  newtypes.
- Auth middleware: session check on `/api/*`, agent-credential check on
  `/agent/*`; handlers never re-implement auth.
- Errors: one `AppError` type implementing `IntoResponse`, producing the
  envelope from `docs/API.md` (`{"error":{"code","message"}}`) with correct
  status. Internals and secrets never appear in messages.

## sqlx / MySQL

- `sqlx::query!`/`query_as!` (compile-checked) by default; dynamic SQL only
  with a stated reason.
- One `MySqlPool` in app state; repositories are plain functions taking
  `&MySqlPool` or `&mut MySqlConnection` — pass the executor so functions
  compose into transactions.
- **Multi-row invariants commit atomically**: deployment state change +
  `deployment_events` row + `audit_logs` row in one transaction, via the
  single transition function per state machine (`docs/RUST_RULES.md`
  § State Machines).
- UUIDs as BINARY(16) behind newtypes; DATETIME(6) UTC; enums stored as the
  exact strings of the `shared` enums.

## Async Discipline

- Never hold a `MutexGuard` across `.await`; prefer bounded `mpsc` channels
  over shared mutable state.
- Long-poll endpoints (`/agent/tasks/next`) use `tokio::select!` with a
  timeout; cancellation-safe.
- `spawn_blocking` for anything blocking; graceful shutdown on SIGTERM.

## Observability

- `tracing` with structured fields:
  `tracing::info!(deployment_id = %id, from = %old, to = %new, "transition")`.
- Span per request (tower-http trace layer); JSON output in production.
- Never log tokens, cookies, or registry credentials.

## Checklist

- [ ] No `unwrap()` on network/DB/user input
- [ ] DTO + enum in `shared/`, not redefined locally
- [ ] Transaction around multi-row state changes
- [ ] Audit row for every state-changing endpoint
- [ ] Auth enforced by middleware, tested
- [ ] `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`
