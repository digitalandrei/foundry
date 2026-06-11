# Phase 3 — Authentication (GitLab OAuth, Multi-Instance)

**Status:** Not started · refine this plan right before starting.

## Goal

Users log in via any onboarded GitLab instance and Foundry resolves their
permissions from GitLab. Implements `../GITLAB-INTEGRATION.md`.

## Deliverables

- `gitlab_instances` admin CRUD (`POST/GET /api/instances`) + onboarding UI
  (Settings), secrets encrypted at rest
- OAuth flow: `/auth/login/{instance}` (PKCE + state) →
  `/auth/callback/{instance}` → session cookie; `/auth/logout`
- `users` + `gitlab_accounts` upsert, token refresh handling
- `GET /api/me`; session middleware protecting `/api/*`
- Permission resolution: `GET /api/projects`, `GET /api/registry`
  (live GitLab queries + short cache + mirror-table sync)
- Login page with instance picker (auto-select when one instance)
- Audit rows for login/onboarding

## Acceptance

- End-to-end login against a real GitLab instance from
  `https://foundry.cloudcraft.ro`; projects/registry listing matches the
  user's GitLab visibility; all auth events audited
