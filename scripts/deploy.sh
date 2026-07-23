#!/usr/bin/env bash
# Canonical production deploy (docs/DEPLOYMENT.md § Deploy Flow). ONE
# command, every time — so the backend AND the frontend are always
# rebuilt from the current tree on a version bump, and the served SPA
# never keeps stale hashed bundles. Run from the repo root; needs sudo.
set -euo pipefail
cd "$(dirname "$0")/.."

SRV=/srv/foundry
SERVICE=foundry-controller

# ── Version sync gate (operator rule: bump both in lockstep) ─────────
cargo_ver=$(grep -m1 '^version = ' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
npm_ver=$(node -p "require('./frontend/package.json').version")
if [ "$cargo_ver" != "$npm_ver" ]; then
  echo "version mismatch: Cargo.toml=$cargo_ver frontend=$npm_ver — bump both together" >&2
  exit 1
fi
echo "▶ deploying v$cargo_ver"

# ── Build (always both) ─────────────────────────────────────────────
echo "▶ building controller + agent (release)…"
# Compile against the checked query metadata. Production is intentionally
# still on the previous schema until the newly built controller starts and
# runs its embedded migrations.
SQLX_OFFLINE=true cargo build --release -p foundry-controller -p foundry-agent
echo "▶ building frontend…"
(cd frontend && npm run build)

# ── Backup: mandatory before the restart can run embedded migrations ─
echo "▶ installing backup job + taking pre-migration backup…"
sudo install -d -m 755 "$SRV/bin"
sudo install -d -m 700 "$SRV/backups/mysql"
sudo install -m 755 scripts/backup.sh "$SRV/bin/foundry-backup"
sudo install -m 644 deployment/systemd/foundry-backup.service \
  /etc/systemd/system/foundry-backup.service
sudo install -m 644 deployment/systemd/foundry-backup.timer \
  /etc/systemd/system/foundry-backup.timer
sudo systemctl daemon-reload
sudo systemctl enable --now foundry-backup.timer
sudo "$SRV/bin/foundry-backup"

# ── Retain live artifacts so scripts/rollback.sh has a target ──────
echo "▶ retaining current artifacts for rollback…"
if [ -x "$SRV/foundry-controller" ]; then
  live_ver=$(curl -fsS http://127.0.0.1:8400/health 2>/dev/null |
    sed -n 's/.*"version":"\([^"]*\)".*/\1/p' || true)
  sudo \cp -f "$SRV/foundry-controller" "$SRV/foundry-controller.prev"
  echo "${live_ver:-unknown}" | sudo tee "$SRV/.prev-version" >/dev/null
fi
if [ -d "$SRV/frontend" ]; then
  sudo \rm -rf "$SRV/frontend.prev"
  sudo \cp -rf "$SRV/frontend" "$SRV/frontend.prev"
fi
if [ -f "$SRV/downloads/foundry-agent" ]; then
  sudo \cp -f "$SRV/downloads/foundry-agent" "$SRV/downloads/foundry-agent.prev"
  [ -f "$SRV/downloads/foundry-agent.sha256" ] &&
    sudo \cp -f "$SRV/downloads/foundry-agent.sha256" "$SRV/downloads/foundry-agent.prev.sha256"
fi

# ── Frontend: replace the tree wholesale (no stale bundles) ─────────
echo "▶ publishing SPA (clean)…"
sudo find "$SRV/frontend" -mindepth 1 -maxdepth 1 -exec \rm -rf {} +
sudo \cp -rf frontend/dist/. "$SRV/frontend/"
sudo chown -R foundry:foundry "$SRV/frontend"

# ── Controller binary + restart ─────────────────────────────────────
echo "▶ installing controller + restarting…"
sudo install -m 755 target/release/foundry-controller "$SRV/foundry-controller"
sudo systemctl restart "$SERVICE"

# ── Agent binary (served to GPU servers) ────────────────────────────
sudo install -m 755 target/release/foundry-agent "$SRV/downloads/foundry-agent"
sha256sum target/release/foundry-agent | sed 's#target/release/##' | sudo tee "$SRV/downloads/foundry-agent.sha256" >/dev/null

# ── Verify ──────────────────────────────────────────────────────────
sleep 2
for _ in $(seq 1 20); do
  if curl -fsS http://127.0.0.1:8400/health 2>/dev/null | grep -q "\"version\":\"$cargo_ver\""; then
    echo "✓ controller live: $(curl -fsS http://127.0.0.1:8400/health)"
    break
  fi
  sleep 0.5
done
live=$(curl -fsS http://127.0.0.1:8400/health 2>/dev/null || echo '{}')
echo "$live" | grep -q "\"version\":\"$cargo_ver\"" || {
  echo "controller did not come up on v$cargo_ver: $live" >&2
  exit 1
}
echo "✓ deployed v$cargo_ver — agent binary at $SRV/downloads/foundry-agent"
