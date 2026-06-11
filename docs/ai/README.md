# Foundry AI Routing

Vendor-neutral routing and knowledge layer for agents working in this repo.
Decide what to load here before touching deep docs or broad areas of the
tree.

## Default Rule

Start with 2–4 targeted files, not the full repo brief.

## Always Load

- `preferences.md` — user profile and behavioral defaults (finish the
  deploy, frontend-first, no god files / reuse-first, docs-in-same-commit).

## Code Tasks

Feature work, bug fixes, review, refactors.

Load order:

1. `codebase-map.md`
2. The relevant specialist in `.claude/agents/`
3. `../RUST_RULES.md` (controller/agent/shared) or `../FRONTEND_RULES.md`
   (frontend)
4. Deep docs only if needed

Specialist routing:

- Controller API, OAuth, scheduler, task queue → `.claude/agents/controller.md`
- GPU-server agent, Docker, NVML → `.claude/agents/gpu-agent.md`
- React UI, shadcn, dnd-kit, theming → `.claude/agents/frontend.md`
- GitLab OAuth/API/registry behavior → `.claude/agents/gitlab-integration.md`
- MySQL schema, migrations, queries → `.claude/agents/mysql-schema.md`
- Docker Engine API + NVIDIA runtime details → `.claude/agents/docker-nvidia.md`
- Auth, tokens, audit, threat-model questions → `.claude/agents/security.md`
- Deploy, systemd, nginx, troubleshooting → `.claude/agents/devops.md`

## Phase Work

Starting or continuing a roadmap phase:

1. `../ROADMAP.md` (current status + amendments log)
2. The phase plan in `../plans/`
3. Then route as a code task

At phase end: update the plan's status, `../ROADMAP.md`, and any doc whose
content the phase changed — same commit set.

## Support / Ops Tasks

Service down, deploy questions, enrollment trouble, GitLab connectivity.

Load order:

1. `product-overview.md`
2. `../DEPLOYMENT.md` (ops playbook, incl. § Runtime Truth)
3. `.claude/agents/devops.md`

Check runtime truth (health endpoint, journald, MySQL, audit log) before
concluding from docs alone.

## Deep Docs

- `../ARCHITECTURE.md` — boundaries, agent protocol, state machines
- `../DATABASE.md` — schema
- `../API.md` — endpoint contracts
- `../GITLAB-INTEGRATION.md` — OAuth, API, registry pulls
- `../GPU-MIG.md` — discovery and slot mechanics
- `../SECURITY.md` — controls and invariants
- `../UI-DESIGN.md` — visual/interaction contract, theming
- `../TESTING.md` — test conventions
