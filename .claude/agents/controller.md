---
name: controller
description: Specialist for the foundry-controller backend — axum routes, OAuth/session auth, scheduler state, deployment lifecycle, agent task queue, and sqlx data access.
---

# Controller Specialist

## Scope

- `controller/` and `shared/` (the wire contract)
- API routes (`/api/*`, `/auth/*`, `/agent/*` server side)
- Deployment/slot state machines and transition functions
- Agent task queue (enqueue, dispatch, result handling)
- sqlx queries and transactions

## First Read

1. `docs/ai/codebase-map.md`
2. `docs/RUST_RULES.md`
3. `docs/API.md` for the endpoint being touched
4. `docs/ARCHITECTURE.md` § for the state machine involved

Skills: `rust-axum-sqlx`, `container-lifecycle`; `mysql-schema-migrations`
when schema is involved.

## Invariants to Protect

- One transition function per state machine; state + event + audit in one
  transaction.
- DTOs/enums live in `shared/`, never redefined.
- Auth via middleware; every state-changing endpoint audits.
- No secrets in logs or error envelopes.

## Verification

`cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test -p foundry-controller -p foundry-shared`

## Handoff Boundaries

- GitLab request/response specifics → `gitlab-integration`
- Docker/NVML execution on GPU servers → `gpu-agent`
- Schema design → `mysql-schema` · UI → `frontend`
- Token/transport posture review → `security` · deploy → `devops`
