# Preferences & Conventions

User preferences and behavioral defaults for agents working in this repo.
Repo-tracked so they travel with `git pull` to any host.

## User Profile

- **Andrei Dinu** (email `andrei.dinu85@gmail.com`). Systems engineer
  running production infrastructure; reads code at the system level — no
  toy explanations.
- Owns this host (Ubuntu 24.04), where the Foundry control plane will run
  behind Nginx at `https://foundry.cloudcraft.ro` (Cloudflare-proxied DNS,
  already configured).

## Behavioral Defaults

### Docs are the spec — keep them exact
The original bootstrap spec was retired in Phase 0; `docs/` is the living
source of truth and **must not drift**. When behavior, schema, API, scope,
or design changes, update the affected docs in the same commit set, and
record scope changes in `../ROADMAP.md` § Amendments Log. After every
phase: update the phase plan status and `../ROADMAP.md`.

### No god files — reuse first
No dumping-ground modules, components, or docs. Small single-responsibility
units; shared logic lives once (`shared/` for Rust wire types,
`frontend/src/lib` + composed components for the UI, one state→color map).
Before writing new code or styles, look for the existing thing to reuse or
extend. **Why:** explicit user requirement (2026-06-11); keeps the codebase
navigable as the workspace grows.

### Frontend-first
A feature with a backend API but no operator-facing view is not done.
Ship the working UI in the same phase as the backend capability.

### Finish the deploy
Once the production service exists (Phase 10+), a change is "done" when it
runs on this host and is verified via `/health` — not when it compiles. If
a deploy step is intentionally skipped, say so explicitly.

### Explicit over clever
Foundry's value is operational clarity: explicit scheduling, explicit state
machines, auditable transitions. Prefer the boring, inspectable design.
No speculative abstractions, no placeholder scaffolding.

### Debug builds during iteration
`cargo build` (debug) while iterating; `--release` for deploys.

### Version sync — bump on every deploy
**Every production deploy increments the minor version** (0.1 → 0.2 →
0.3; user rule 2026-06-11) so the operator always knows the running
build: `Cargo.toml` workspace version and `frontend/package.json` move
together. Visible at `/health` (controller), `foundry-agent --version`
+ heartbeat (agent, shown on the Servers page), and the dashboard
sidebar (frontend).

## Project Invariants (quick recall)

- Pull-only agents; controller never connects to GPU servers; no SSH; no
  remote Docker socket.
- Foundry only acts on containers it created (`foundry.managed=true`) **or**
  ones an operator has explicitly **adopted** (a deployment row with
  `adopted_container_id`, resolved by docker id). It never mutates a foreign
  container blind; adopting + destructive ops on adopted containers are
  audited and double-confirmed in the UI.
- Slots are UUID-addressed; never GPU indexes.
- GitLab is the source of truth for permissions; multiple instances
  supported.
- Every state transition is audited (event row + audit row, one
  transaction).
- Dark mode default, light mode required; semantic color tokens only.

## Host Specifics (this Linux host)

- `cp`/`rm` are aliased to `-i` here — use `\cp -f` / `\rm -f` in scripts
  and tool calls.
- nftables (not ufw/iptables) for firewall topics.
- Planned service layout: binary at `/srv/foundry/`, unit
  `foundry-controller.service`, env at `/srv/foundry/.env`
  (see `../DEPLOYMENT.md`).

## Keeping This Honest

These docs and the rule sets under `.claude/` are prompts that shape agent
behavior every session. When you change behavior captured here, update the
file in the same commit set — the Stop hook
(`.claude/hooks/doc-drift-check.sh`) nudges when watched code paths change
without a docs change. It informs; it does not block.

- **Deploy with `scripts/deploy.sh`** (the one canonical path): it
  gates on Cargo.toml ↔ frontend/package.json version parity, rebuilds
  *both* backend and frontend on every deploy, and replaces the served
  SPA wholesale so no stale hashed bundles accumulate. A version bump
  always reships the GUI.
