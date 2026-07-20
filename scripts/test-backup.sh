#!/usr/bin/env bash
# Hermetic smoke test for backup atomicity, validation, permissions, and
# retention. A real dump/restore round-trip runs in the MariaDB CI job.
set -euo pipefail
cd "$(dirname "$0")/.."

tmp_root=$(mktemp -d)
trap 'rm -rf "$tmp_root"' EXIT
mkdir -p "$tmp_root/bin" "$tmp_root/backups"

cat >"$tmp_root/client.cnf" <<'EOF'
[client]
user=backup-test
password=must-not-appear-in-output
EOF
chmod 600 "$tmp_root/client.cnf"

cat >"$tmp_root/bin/mariadb-dump" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' \
  '-- MariaDB dump 10.19  Distrib 11.4, for Linux (x86_64)' \
  'CREATE DATABASE IF NOT EXISTS `foundry`;' \
  'USE `foundry`;' \
  'CREATE TABLE backup_probe (id INT PRIMARY KEY);' \
  'INSERT INTO backup_probe VALUES (1);'
EOF
chmod 755 "$tmp_root/bin/mariadb-dump"

for _ in 1 2 3; do
  output=$(
    PATH="$tmp_root/bin:$PATH" \
      FOUNDRY_BACKUP_DEFAULTS_FILE="$tmp_root/client.cnf" \
      FOUNDRY_BACKUP_DIR="$tmp_root/backups" \
      FOUNDRY_BACKUP_KEEP=2 \
      scripts/backup.sh
  )
  if [[ "$output" == *must-not-appear-in-output* ]]; then
    echo "backup leaked credentials" >&2
    exit 1
  fi
done

mapfile -t archives < <(find "$tmp_root/backups" -type f -name 'foundry-*.sql.gz' | sort)
[[ ${#archives[@]} -eq 2 ]] || {
  echo "retention test expected 2 archives, found ${#archives[@]}" >&2
  exit 1
}
for archive in "${archives[@]}"; do
  gzip -t "$archive"
  [[ $(stat -c '%a' "$archive") == 600 ]] || {
    echo "archive is not mode 0600: $archive" >&2
    exit 1
  }
  gzip -cd "$archive" | grep -q 'INSERT INTO backup_probe VALUES (1);'
done

if find "$tmp_root/backups" -type f -name '*.tmp' | grep -q .; then
  echo "backup left a temporary file behind" >&2
  exit 1
fi

echo "backup smoke test passed"
