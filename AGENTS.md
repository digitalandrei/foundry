# Foundry Agent Bootstrap

Canonical bootstrap for agentic work in this repository.

## Goal

Load the smallest accurate context that can solve the task. Start with the
routing layer in `docs/ai/`, then load only what the task needs.

## First Step

1. Read `docs/ai/README.md`.
2. Classify the task:
   - code implementation, debugging, or review
   - roadmap phase work (starting/continuing a phase)
   - support, deployment, or runtime troubleshooting
3. Follow the matching load order from `docs/ai/README.md`.

## Default Load Orders

### Code tasks
1. `docs/ai/codebase-map.md`
2. The relevant specialist in `.claude/agents/`
3. `docs/RUST_RULES.md` or `docs/FRONTEND_RULES.md`
4. Targeted deep docs only if needed

### Phase work
1. `docs/ROADMAP.md` (status + amendments log)
2. The phase plan in `docs/plans/`
3. Continue as a code task

### Support and ops tasks
1. `docs/ai/product-overview.md`
2. `docs/DEPLOYMENT.md`
3. `.claude/agents/devops.md`
4. Check runtime truth (health, journald, MySQL, audit log) before
   concluding from docs

## Specialist Map

- `.claude/agents/controller.md` — controller API, lifecycle, task queue, sqlx
- `.claude/agents/gpu-agent.md` — agent loop, Docker executors, NVML inventory
- `.claude/agents/frontend.md` — React UI, dnd-kit, theming
- `.claude/agents/gitlab-integration.md` — OAuth, GitLab API, registry tokens
- `.claude/agents/mysql-schema.md` — schema, migrations
- `.claude/agents/docker-nvidia.md` — GPU assignment, Container Toolkit, MIG
- `.claude/agents/security.md` — auth posture, secrets, audit integrity
- `.claude/agents/devops.md` — nginx/Cloudflare, systemd, ops

## Deep References (load on need)

- `docs/ARCHITECTURE.md` — boundaries, agent protocol, state machines
- `docs/DATABASE.md` · `docs/API.md` · `docs/GITLAB-INTEGRATION.md`
- `docs/GPU-MIG.md` · `docs/SECURITY.md` · `docs/DEPLOYMENT.md`
- `docs/UI-DESIGN.md` · `docs/TESTING.md`

## Non-Negotiables

- **Docs are the spec.** The original bootstrap spec was retired; when
  behavior/schema/scope changes, update the affected docs and
  `docs/ROADMAP.md` in the same commit set.
- **No god files; reuse first.** Small single-responsibility modules;
  shared logic lives once (`shared/`, `frontend/src/lib`, composed
  components).
- Project invariants: see `docs/ai/preferences.md` § Project Invariants.
