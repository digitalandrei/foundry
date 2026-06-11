# Migrations

sqlx MySQL migrations, applied with `sqlx migrate run` (reads
`DATABASE_URL` from `.env`). Forward-only; conventions in the
`mysql-schema-migrations` skill. The human-readable schema contract is
`docs/DATABASE.md` — update it in the same commit as any migration.

The initial schema (19 tables) lands in Phase 2.
