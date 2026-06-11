# Phase 3 — Authentication (GitLab OAuth, Multi-Instance)

**Status:** 🔶 Built & deployed (2026-06-11) — closing item: E2E login
against the first real GitLab instance (waiting on instance details +
OAuth app from the operator; see ROADMAP amendments for what changed).

Delivered: sessions table + hashed-token sessions (7d, sweeper);
AES-256-GCM secrets at rest; OAuth PKCE flow with encrypted state
cookie and **fixed** `/auth/callback`; token refresh; `/api/me`,
`/api/instances` (+`/full`, admin POST), `/api/projects` (live,
degrades per instance), `/api/registry/{project}` (lazy tree, tag
size/date); audit rows for login/logout/onboarding; bootstrap CLI;
frontend login picker, session guard, user menu, registry sidebar
tree, admin instance onboarding (RHF+zod+Field); production deploy at
https://foundry.cloudcraft.ro (nginx static SPA + API proxy,
self-signed origin cert behind Cloudflare Full).

Extension (user request, same day): **in-app help page**
`/help/gitlab-oauth` — OAuth app setup steps + scope rationale
(5 required read-only scopes, leave-unchecked list), linked from the
top-nav help icon and the onboarding form.

Extension (user request, same day): **local operator accounts** —
argon2id credentials, `POST /auth/local`, operator form on the login
page, `admin add`/`admin set-password` CLI; first `admin` account live
on production. Verified end-to-end through https (wrong-password 401,
login, admin endpoint access, logout, audit rows).

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
