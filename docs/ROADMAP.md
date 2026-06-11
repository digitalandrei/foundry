# Foundry Roadmap

Live progress tracker. **Update the status column at the end of every
phase, in the same commit set as the work.** Detailed per-phase plans live
in `docs/plans/`.

| Phase | Title | Plan | Status |
|---|---|---|---|
| 0 | Documentation & AI tooling bootstrap | (this work) | ✅ Done (2026-06-11) |
| 1 | Repository bootstrap | [plans/phase-01.md](plans/phase-01.md) | ✅ Done (2026-06-11) |
| 2 | Workspace creation | [plans/phase-02.md](plans/phase-02.md) | ✅ Done (2026-06-11) |
| 3 | Authentication (GitLab OAuth, multi-instance) | [plans/phase-03.md](plans/phase-03.md) | 🔶 Built & deployed — E2E login awaits the first GitLab instance |
| 4 | Agent enrollment | [plans/phase-04.md](plans/phase-04.md) | ⬜ Not started |
| 5 | Inventory (GPU/MIG discovery & reconciliation) | [plans/phase-05.md](plans/phase-05.md) | ⬜ Not started |
| 6 | Deployments (lifecycle, replacement) | [plans/phase-06.md](plans/phase-06.md) | ⬜ Not started |
| 7 | Logs | [plans/phase-07.md](plans/phase-07.md) | ⬜ Not started |
| 8 | UI (full dashboard, dark+light themes) | [plans/phase-08.md](plans/phase-08.md) | ⬜ Not started |
| 9 | Security hardening | [plans/phase-09.md](plans/phase-09.md) | ⬜ Not started |
| 10 | Production readiness | [plans/phase-10.md](plans/phase-10.md) | ⬜ Not started |

## Success Criteria (v1 done)

A user can:

1. Login with GitLab (any onboarded instance)
2. View authorized registries
3. View servers
4. View GPUs
5. View MIGs
6. Deploy containers (drag & drop)
7. Replace containers (with confirmation)
8. View logs
9. Audit actions
10. Operate without SSH

## Status Legend

⬜ Not started · 🔶 In progress · ✅ Done

## Amendments Log

Scope/architecture changes agreed after the original spec — each must be
reflected in the affected docs in the same commit set:

- **2026-06-11** — Multi-GitLab-instance support (instances onboarded
  dynamically; login per instance). Affects ARCHITECTURE, DATABASE
  (`gitlab_instances`), API, GITLAB-INTEGRATION, phase 3.
- **2026-06-11** — Original bootstrap spec retired; these docs are the
  living source of truth. Features may be added/removed/modified here.
- **2026-06-11** — UI: dark mode default per approved mockup; light mode
  required. GitLab browsing lives in the dashboard sidebar, not separate
  pages.
- **2026-06-11** (Phase 1) — Database server is **MariaDB 11.4** on this
  host, not MySQL 8.x; sqlx's MySQL driver targets it. DB `foundry` +
  scoped user provisioned (DEPLOYMENT.md § MySQL).
- **2026-06-11** (Phase 1, confirmed Phase 2) — **No CI.** Deploying is
  easy enough from this host; `scripts/check.sh` is the verification
  gate, run locally before every commit.
- **2026-06-11** (Phase 2) — Controller binds `127.0.0.1:8400` by
  default (8080 is taken on this host). Migrations are embedded in the
  controller and run at startup.
- **2026-06-11** (Phase 2) — Frontend theming via `next-themes`
  (already a shadcn/sonner dependency — reuse over a hand-rolled
  provider); storage key `foundry-theme`, dark default.
- **2026-06-11** — A separate test host (Docker, **read-only for now**)
  is available for deploying the agent against real containers — to be
  wired in during Phases 4–5 (enrollment + inventory are exactly the
  read-only surface). Connection details to be provided at Phase 4
  start.
- **2026-06-11** (Phase 3) — **OAuth over PATs**: portal-triggered
  GitLab OAuth is the only v1 login method; self-generated read-only
  PATs stay documented as a future fallback
  (GITLAB-INTEGRATION.md § Multi-Instance Model).
- **2026-06-11** (Phase 3) — **One fixed OAuth redirect URI**
  (`/auth/callback`) for all instances; pending-login state rides in an
  encrypted cookie. Replaces the spec's `/auth/callback/{instance}`.
- **2026-06-11** (Phase 3) — `sessions` table added (server-side
  sessions, hashed tokens). DATABASE.md now counts 20 tables.
- **2026-06-11** (Phase 3) — **Went live early** (user-approved; spec
  put this in Phase 10): controller systemd service + Nginx vhost +
  static SPA at `https://foundry.cloudcraft.ro`. **Serving model
  decided**: Nginx serves the frontend statically, controller is
  API-only (no rust-embed) — closes the Phase 8 decision point.
- **2026-06-11** (Phase 3) — First-instance bootstrap CLI:
  `foundry-controller instance add` (Settings UI requires an admin,
  who requires a login, which requires an instance).
- **2026-06-11** (Phase 3) — **Local operator accounts** (user
  request): username/argon2id-password logins (`local_credentials`,
  21 tables now) for GitLab-independent administration. CLI-managed
  (`admin add` / `admin set-password`), always `is_admin`, no GitLab
  identity → no project/registry/deploy rights. `POST /auth/local` +
  operator form on the login page. First account `admin` created on
  production.
