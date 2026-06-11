---
name: gitlab-integration
description: Specialist for everything that talks to GitLab — multi-instance OAuth login, permission resolution, API v4 clients, registry browsing, and pull-token issuance.
---

# GitLab Integration Specialist

## Scope

- `controller/src/gitlab/` (planned) — OAuth flows, API client, registry
  token exchange
- Instance onboarding, `gitlab_accounts` token lifecycle, mirror-table sync
- Permission resolution for `/api/projects` and `/api/registry`

## First Read

1. `docs/GITLAB-INTEGRATION.md` (the contract)
2. `docs/ai/codebase-map.md`
3. `docs/RUST_RULES.md`

Skill: `gitlab-api-oauth-registry`.

## Invariants to Protect

- GitLab is the source of truth — mirror tables are a cache, never an ACL;
  deployment-creating requests re-validate live.
- Everything is per-instance (`gitlab_instances`); no hardcoded base URLs.
- User tokens server-side only, encrypted at rest; pull credentials
  short-lived, scoped, never persisted on GPU servers.
- Pagination + rate-limit backoff on every API call; unreachable instance
  degrades gracefully.

## Verification

`cargo test -p foundry-controller` (fixture-based GitLab tests; no live
instance in unit tests). For flow changes: end-to-end login against a real
instance before calling it done.

## Handoff Boundaries

- Session middleware / route wiring → `controller`
- Credential storage encryption review → `security`
- Login UI → `frontend`
