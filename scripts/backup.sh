#!/usr/bin/env bash
# Atomic local MariaDB backup for Foundry. Credentials come exclusively from
# a root-readable MariaDB option file; they are never exposed in argv or logs.
set -euo pipefail

DEFAULTS_FILE=${FOUNDRY_BACKUP_DEFAULTS_FILE:-/etc/foundry/mysql-backup.cnf}
BACKUP_DIR=${FOUNDRY_BACKUP_DIR:-/srv/foundry/backups/mysql}
DATABASE=${FOUNDRY_BACKUP_DATABASE:-foundry}
KEEP=${FOUNDRY_BACKUP_KEEP:-10}

if [[ ! "$KEEP" =~ ^[1-9][0-9]*$ ]]; then
  echo "FOUNDRY_BACKUP_KEEP must be a positive integer" >&2
  exit 2
fi
if [[ ! -r "$DEFAULTS_FILE" ]]; then
  echo "backup credentials are not readable: $DEFAULTS_FILE" >&2
  exit 1
fi

umask 077
install -d -m 700 "$BACKUP_DIR"
exec 9>"$BACKUP_DIR/.backup.lock"
if ! flock -n 9; then
  echo "another Foundry backup is already running" >&2
  exit 1
fi

stamp=$(date -u +%Y%m%dT%H%M%S.%NZ)
final="$BACKUP_DIR/foundry-$stamp.sql.gz"
tmp=$(mktemp "$BACKUP_DIR/.foundry-$stamp.XXXXXX.tmp")
trap 'rm -f "$tmp"' EXIT

mariadb-dump \
  --defaults-extra-file="$DEFAULTS_FILE" \
  --single-transaction \
  --quick \
  --routines \
  --events \
  --triggers \
  --hex-blob \
  --databases "$DATABASE" | gzip -9 >"$tmp"

[[ -s "$tmp" ]] || { echo "backup produced an empty archive" >&2; exit 1; }
gzip -t "$tmp"
if ! zgrep -aqm1 '^-- MariaDB dump' "$tmp"; then
  echo "backup archive does not contain a MariaDB dump header" >&2
  exit 1
fi
chmod 600 "$tmp"
sync -f "$tmp"
mv "$tmp" "$final"
sync -f "$BACKUP_DIR"
trap - EXIT

mapfile -t backups < <(find "$BACKUP_DIR" -maxdepth 1 -type f -name 'foundry-*.sql.gz' -printf '%f\n' | sort -r)
if (( ${#backups[@]} > KEEP )); then
  for old in "${backups[@]:KEEP}"; do
    rm -f -- "$BACKUP_DIR/$old"
  done
fi

echo "backup complete: $final"
