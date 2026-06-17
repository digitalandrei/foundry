#!/usr/bin/env bash
# Apply a single migration file to the dev DB AND record it in
# _sqlx_migrations exactly as `sqlx::migrate!` would (version = numeric
# filename prefix, description = remaining words, checksum = sha384 of the
# file). Lets compile-time `sqlx::query!` checks see new columns without
# sqlx-cli, while keeping the controller's boot-time migrator happy.
#
# Usage: scripts/apply-migration.sh migrations/2026..._name.sql
set -euo pipefail
cd "$(dirname "$0")/.."
set -a && . ./.env && set +a

file="$1"
base="$(basename "$file" .sql)"
version="${base%%_*}"
description="$(echo "${base#*_}" | tr '_' ' ')"
checksum="$(sha384sum "$file" | awk '{print $1}')"

DB() { mysql -u foundry -p"$FOUNDRY_DB_PASS" foundry "$@"; }

if DB -N -e "SELECT 1 FROM _sqlx_migrations WHERE version=$version" | grep -q 1; then
  echo "migration $version already recorded — skipping"
  exit 0
fi

echo "applying $file ..."
DB < "$file"
DB -e "INSERT INTO _sqlx_migrations
         (version, description, success, checksum, execution_time)
       VALUES
         ($version, '$description', 1, UNHEX('$checksum'), 0);"
echo "recorded migration $version ($description)"
