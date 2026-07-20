#!/usr/bin/env bash
# The standard verification gate (docs/RUST_RULES.md § Testing & Tooling).
# Run before claiming any change complete.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo deny check advisories
bash scripts/test-backup.sh

# Frontend gate activates once the Vite app exists (Phase 2).
if [ -f frontend/package.json ]; then
  (cd frontend && npm audit --omit=dev --audit-level=high && npm run lint && npm run test:run && npm run build)
fi

echo "check.sh: all gates passed"
