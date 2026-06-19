# Plan 004: Add `.env.example` and a standalone frontend `typecheck` script

> **Executor instructions**: Follow this plan step by step. Run every
> verification command. If a STOP condition occurs, stop and report. When done,
> update this plan's row in `advisor-plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 14f9d95..HEAD -- controller/src/config.rs frontend/package.json`
> If `config.rs` env parsing changed, re-derive the variable list from the live
> code before writing `.env.example`.

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: dx
- **Planned at**: commit `14f9d95`, 2026-06-19

## Why this matters

Two small onboarding-friction gaps:

1. The controller reads its entire configuration from environment variables
   (`controller/src/config.rs`), but there is **no `.env.example`** and no
   single place documenting which vars are required vs optional or their
   formats. A fresh checkout (or a fresh production host) has to read
   `config.rs` to discover that `FOUNDRY_ENCRYPTION_KEY` is mandatory — and a
   missing one fails at startup with a bare `Missing: FOUNDRY_ENCRYPTION_KEY`.
2. The frontend has **no standalone typecheck** — `tsc` only runs as part of
   `npm run build` (`"build": "tsc -b && vite build"`), so catching a type
   error means waiting for a full bundle. A `tsc --noEmit` script gives a fast
   feedback loop and a cheap gate for CI (plan 006).

Neither is urgent; both are cheap and remove real friction the operator and any
executor hit on day one.

## Current state

`controller/src/config.rs` reads these vars (lines 39-70). Required vs optional
and defaults, derived from the live code:

| Var | Required? | Default / format |
|-----|-----------|------------------|
| `DATABASE_URL` | **required** | `mysql://user:pass@host/foundry` |
| `FOUNDRY_ENCRYPTION_KEY` | **required** | base64 of 32 random bytes (AES-256-GCM key) |
| `FOUNDRY_BIND` | optional | socket addr; default `127.0.0.1:8400` |
| `FOUNDRY_DB_MAX_CONNECTIONS` | optional | integer; default `10` |
| `FOUNDRY_PUBLIC_URL` | optional | default `https://foundry.cloudcraft.ro` |
| `FOUNDRY_APPS_DOMAIN` | optional | apps wildcard domain; lower-cased, dot-trimmed |
| `FOUNDRY_ADMIN_EMAILS` | optional | comma-separated emails |

`.env` is gitignored (`.gitignore` has `.env` and `.env.*`) and holds the real
secrets — it is correctly **not** committed. `.env.example` carries placeholders
only and IS committed.

`frontend/package.json` scripts (no typecheck):

```json
"scripts": { "dev": "vite", "build": "tsc -b && vite build", "lint": "eslint .", "preview": "vite preview" }
```

## Commands you will need

| Purpose | Command | Expected |
|---------|---------|----------|
| Frontend typecheck (after add) | `cd frontend && npm run typecheck` | exit 0, no emit |
| Confirm no secrets in example | `grep -nE '=[^[:space:]]+' .env.example` | only placeholder values |

## Scope

**In scope**:
- `.env.example` (new, repo root)
- `frontend/package.json` (add `typecheck` script)
- `scripts/check.sh` (optional: run `npm run typecheck` before build) — only if
  it doesn't duplicate work `build` already does; see Step 3
- `docs/DEPLOYMENT.md` (add a short "Environment variables" reference pointing
  at `.env.example`)

**Out of scope**:
- The real `.env` — never read its values into `.env.example`; never commit it.
- The agent's TOML config (`agent/src/config.rs`, `FOUNDRY_AGENT_CONFIG`) — the
  agent is not configured via `.env`; do not invent agent env vars.

## Git workflow

- Branch: `advisor/004-dx-env-typecheck`
- Commits: one for `.env.example` + docs, one for the frontend script.
- Do NOT push unless instructed.

## Steps

### Step 1: Write `.env.example`

Create `.env.example` at the repo root with **placeholder** values only, one
commented line per var noting required/optional, matching the table above. For
the encryption key, the placeholder must make the format obvious without being
a usable key, e.g.:

```
# Required. AES-256-GCM key: base64 of 32 random bytes.
# Generate with: openssl rand -base64 32
FOUNDRY_ENCRYPTION_KEY=replace-me-base64-32-bytes

# Required. MySQL DSN for the foundry database.
DATABASE_URL=mysql://foundry:CHANGEME@127.0.0.1/foundry

# Optional (defaults shown).
FOUNDRY_BIND=127.0.0.1:8400
FOUNDRY_DB_MAX_CONNECTIONS=10
FOUNDRY_PUBLIC_URL=https://foundry.example.com
# FOUNDRY_APPS_DOMAIN=apps.example.com
# FOUNDRY_ADMIN_EMAILS=admin@example.com,ops@example.com
```

**Verify**: `git check-ignore .env.example` → prints nothing (i.e. the example
is NOT ignored and will be committed); `git check-ignore .env` → prints `.env`
(the real one stays ignored).

### Step 2: Add the frontend `typecheck` script

Add to `frontend/package.json` scripts:

```json
"typecheck": "tsc --noEmit"
```

**Verify**: `cd frontend && npm run typecheck` → exit 0.

### Step 3: (Optional) reference it where it helps

- Add a short "Environment variables" subsection to `docs/DEPLOYMENT.md` that
  says "copy `.env.example` to `/srv/foundry/.env` and fill in the required
  vars" and points at the table.
- Do NOT add `typecheck` to `scripts/check.sh` if `build` already runs `tsc -b`
  there (it does) — that would double-typecheck. The standalone script is for
  fast local iteration and for CI's cheap gate (plan 006). Leave `check.sh`'s
  frontend line as-is unless plan 003 changed it.

**Verify**: `bash scripts/check.sh` → "check.sh: all gates passed".

## Done criteria

- [ ] `.env.example` exists at repo root and is tracked
      (`git check-ignore .env.example` prints nothing)
- [ ] `.env.example` contains no real secret (only placeholders)
- [ ] `.env` is still ignored (`git check-ignore .env` prints `.env`)
- [ ] `cd frontend && npm run typecheck` exits 0
- [ ] `bash scripts/check.sh` passes
- [ ] `advisor-plans/README.md` status row updated

## STOP conditions

- `config.rs` reads a var not in the table above (the live code drifted) — add
  it to `.env.example` and note the discrepancy in your report.
- You are about to copy any value out of the real `.env` — STOP; the example
  takes placeholders only.

## Maintenance notes

- When a new `FOUNDRY_*` var is added to `config.rs`, add it to `.env.example`
  in the same commit — same docs-are-the-spec discipline as the codebase map.
- CI (plan 006) should call `npm run typecheck` as a fast pre-build gate.
