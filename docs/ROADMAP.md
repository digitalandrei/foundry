# Foundry Roadmap

Live progress tracker. **Update the status column at the end of every
phase, in the same commit set as the work.** Detailed per-phase plans live
in `docs/plans/`.

| Phase | Title | Plan | Status |
|---|---|---|---|
| 0 | Documentation & AI tooling bootstrap | (this work) | ✅ Done (2026-06-11) |
| 1 | Repository bootstrap | [plans/phase-01.md](plans/phase-01.md) | ⬜ Not started |
| 2 | Workspace creation | [plans/phase-02.md](plans/phase-02.md) | ⬜ Not started |
| 3 | Authentication (GitLab OAuth, multi-instance) | [plans/phase-03.md](plans/phase-03.md) | ⬜ Not started |
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
