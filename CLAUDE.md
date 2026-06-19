# Foundry Claude Adapter

This file is intentionally thin. Knowledge lives in the shared routing
layer, not here.

## Start Here

1. Read `AGENTS.md`.
2. Read `docs/ai/README.md`.
3. Classify the task and load only the relevant specialist + deep docs.

Do not treat this file as a monolithic first-read brief.

## Specialist Files

`.claude/agents/`: controller, gpu-agent, frontend, gitlab-integration,
mysql-schema, docker-nvidia, security, devops, architect (end-of-session
audit + handoff plans, via the `improve` skill).

## Shared Knowledge

- `docs/ai/preferences.md` — user prefs + behavioral defaults (docs are
  the spec, no god files / reuse first, frontend-first, finish the
  deploy). **Load on every fresh session.**
- `docs/ai/codebase-map.md` — file routing
- `docs/ai/product-overview.md` — what Foundry does
- `docs/ROADMAP.md` — phase status + amendments log

## Hard Rules

- **Doc maintenance**: the docs under `docs/` and the rule sets under
  `.claude/` are prompts; when you change behavior they describe, update
  them in the same commit set. A Stop hook
  (`.claude/hooks/doc-drift-check.sh`, wired in `.claude/settings.json`)
  nudges when watched code paths change without a matching docs change —
  it informs, it does not block.
- **Phase tracking**: at the end of any roadmap phase work, update the
  phase plan in `docs/plans/` and `docs/ROADMAP.md`.
- This host aliases `cp`/`rm` to `-i` — use `\cp -f` / `\rm -f`.

## Permissions

`.claude/settings.json` is a permissions policy, not a product brief.
