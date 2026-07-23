#!/usr/bin/env bash
# Roll back to the previously deployed controller + SPA + agent binary
# retained by scripts/deploy.sh (docs/DEPLOYMENT.md § Rollback). Run
# from the repo root; needs sudo. Holds exactly one generation.
#
# Migrations are forward-only: if the deploy being rolled back applied
# new migrations, restore the pre-migration DB backup FIRST (restore
# procedure in docs/DEPLOYMENT.md § Deploy Flow) — the previous binary
# refuses to start against a schema newer than its embedded migrations.
set -euo pipefail
cd "$(dirname "$0")/.."

SRV=/srv/foundry
SERVICE=foundry-controller

[ -x "$SRV/foundry-controller.prev" ] || {
  echo "no retained previous controller at $SRV/foundry-controller.prev — nothing to roll back to" >&2
  exit 1
}
[ -d "$SRV/frontend.prev" ] || {
  echo "no retained previous SPA at $SRV/frontend.prev — nothing to roll back to" >&2
  exit 1
}
prev_ver=$(cat "$SRV/.prev-version" 2>/dev/null || echo unknown)
echo "▶ rolling back to v$prev_ver"

# Controller binary + SPA tree (wholesale, no stale bundles), then restart.
sudo install -m 755 "$SRV/foundry-controller.prev" "$SRV/foundry-controller"
sudo find "$SRV/frontend" -mindepth 1 -maxdepth 1 -exec \rm -rf {} +
sudo \cp -rf "$SRV/frontend.prev/." "$SRV/frontend/"
sudo chown -R foundry:foundry "$SRV/frontend"
sudo systemctl restart "$SERVICE"

# Served agent binary, if a previous generation was retained.
if [ -f "$SRV/downloads/foundry-agent.prev" ]; then
  sudo install -m 755 "$SRV/downloads/foundry-agent.prev" "$SRV/downloads/foundry-agent"
  [ -f "$SRV/downloads/foundry-agent.prev.sha256" ] &&
    sudo \cp -f "$SRV/downloads/foundry-agent.prev.sha256" "$SRV/downloads/foundry-agent.sha256"
fi

# ── Verify ──────────────────────────────────────────────────────────
sleep 2
live='{}'
for _ in $(seq 1 20); do
  live=$(curl -fsS http://127.0.0.1:8400/health 2>/dev/null || echo '{}')
  if [ "$prev_ver" != unknown ]; then
    echo "$live" | grep -q "\"version\":\"$prev_ver\"" && break
  else
    echo "$live" | grep -q '"status":"ok"' && break
  fi
  sleep 0.5
done
if [ "$prev_ver" != unknown ]; then
  echo "$live" | grep -q "\"version\":\"$prev_ver\"" || {
    echo "controller did not come back on v$prev_ver: $live" >&2
    echo "if the failed deploy applied migrations, restore the pre-migration backup (docs/DEPLOYMENT.md § Deploy Flow) and re-run" >&2
    exit 1
  }
else
  echo "$live" | grep -q '"status":"ok"' || {
    echo "controller did not come back healthy: $live" >&2
    exit 1
  }
fi
echo "✓ rolled back — controller live: $live"
