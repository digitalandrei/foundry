---
name: mysql-schema-migrations
description: >
  For the Foundry project at /opt/foundry. MySQL schema design and sqlx
  migration discipline. Use when creating or altering tables, writing
  migrations, or reasoning about indexes, constraints, and schema/Rust-enum
  sync.
---

# MySQL Schema & Migrations

Schema contract: `docs/DATABASE.md` (update it in the same commit as any
migration). Migrations live in `migrations/`, applied with
`sqlx migrate run`.

## Migration Discipline

- **Forward-only.** No down migrations; a mistake is fixed by a new
  migration.
- Naming: `YYYYMMDDHHMMSS_short_description.sql` (sqlx default).
- One concern per migration; never edit an applied migration.
- Destructive migrations (drop/modify columns with data) require a
  pre-migration backup step documented in `docs/DEPLOYMENT.md` § MySQL.
- After schema changes, keep sqlx offline data in sync so CI compiles
  (`cargo sqlx prepare`).

## Schema Conventions

- PK: `id BINARY(16)` (UUIDv7) — newtype-wrapped in Rust.
- Timestamps: `created_at`/`updated_at DATETIME(6)`, UTC, app-managed.
- State columns: `VARCHAR` holding exactly the `shared/` enum strings —
  the Rust enum is the source of truth; add enum variants and migration in
  the same change.
- `utf8mb4` everywhere; explicit FK constraints; name them
  (`fk_<table>_<ref>`).
- Index every FK and every column used in list filters
  (state, server_id, created_at desc for feeds).
- Append-only tables (`deployment_events`, `audit_logs`): INSERT only —
  no UPDATE/DELETE statements against them in application code, ever.
- Secrets at rest (tokens, oauth secrets, secret env values): encrypted by
  the app before INSERT — columns are `VARBINARY`/`BLOB`, never plaintext.

## Query Practices

- Compile-checked `sqlx::query!`; pass executors so repo functions compose
  into transactions.
- Pagination on every list query (`LIMIT/OFFSET` with caps per
  `docs/API.md`); no unbounded SELECTs on mirror or audit tables.
