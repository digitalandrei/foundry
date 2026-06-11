#!/usr/bin/env bash
# The standard verification gate (docs/RUST_RULES.md § Testing & Tooling).
# Run before claiming any change complete.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test

# Frontend gate activates once the Vite app exists (Phase 2).
if [ -f frontend/package.json ]; then
  (cd frontend && npm run build)
fi

echo "check.sh: all gates passed"
