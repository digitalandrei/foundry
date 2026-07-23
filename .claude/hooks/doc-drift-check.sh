#!/usr/bin/env bash
# .claude/hooks/doc-drift-check.sh
#
# Stop hook for the foundry repo.
# Looks at the unpushed change set (committed + staged + working tree) and
# warns when code in a watched area changed without a corresponding update
# to the doc that describes its behavior.
#
# Output is surfaced back to Claude as additional context after a turn ends.
# Silent when nothing is amiss. A nudge, not an enforcement.

set -eu

cd "${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}" 2>/dev/null || exit 0

# Gather everything that hasn't reached origin/main yet.
files=$( {
  git log --name-only --pretty=format: origin/main..HEAD 2>/dev/null || true
  git diff --name-only HEAD 2>/dev/null || true
  git diff --name-only --cached 2>/dev/null || true
} | sort -u | sed '/^$/d')

[ -z "$files" ] && exit 0

touched() { echo "$files" | grep -qE "$1"; }

warnings=()

# Controller routes / lifecycle / task queue / feature repos ↔ API.md, ARCHITECTURE.md, controller agent
if touched '^controller/src/(routes/|lifecycle\.rs|files\.rs|shell\.rs|repos/(tasks|deployments|volumes|gpu_groups|logs|metrics)\.rs)'; then
  touched '^docs/(API|ARCHITECTURE)\.md|^\.claude/agents/controller\.md' || \
    warnings+=("controller routes/lifecycle/tasks/feature repos changed; review docs/API.md, docs/ARCHITECTURE.md (state machines), and .claude/agents/controller.md.")
fi

# Security-bearing controller code ↔ SECURITY.md
if touched '^controller/src/(auth/|crypto\.rs|audit\.rs)'; then
  touched '^docs/SECURITY\.md|^\.claude/agents/security\.md' || \
    warnings+=("auth/crypto/audit code changed; review docs/SECURITY.md (controls + invariants) and .claude/agents/security.md.")
fi

# GitLab client code ↔ GITLAB-INTEGRATION.md
if touched '^controller/src/gitlab/'; then
  touched '^docs/GITLAB-INTEGRATION\.md|^\.claude/skills/gitlab-api-oauth-registry/' || \
    warnings+=("GitLab client code changed; review docs/GITLAB-INTEGRATION.md and the gitlab-api-oauth-registry skill.")
fi

# Shared contract (enums/DTOs) ↔ ARCHITECTURE state machines, DATABASE enum strings, frontend types
if touched '^shared/src/'; then
  touched '^docs/(ARCHITECTURE|DATABASE|API)\.md|^frontend/src/lib/' || \
    warnings+=("shared/ wire contract changed; review docs/ARCHITECTURE.md, docs/DATABASE.md (enum strings), docs/API.md, and the frontend type mirror in frontend/src/lib/.")
fi

# Agent code ↔ GPU-MIG.md, agent skills
if touched '^agent/src/'; then
  touched '^docs/(GPU-MIG|ARCHITECTURE)\.md|^\.claude/(agents/gpu-agent\.md|skills/(docker-engine-api|nvidia-gpu-mig|https-mtls-agent-transport)/)' || \
    warnings+=("agent code changed; review docs/GPU-MIG.md, docs/ARCHITECTURE.md (agent protocol), and the gpu-agent specialist/skills.")
fi

# Migrations ↔ DATABASE.md
if touched '^migrations/'; then
  touched '^docs/DATABASE\.md' || \
    warnings+=("migrations/ changed without docs/DATABASE.md — the schema doc must move in the same commit set.")
fi

# Frontend ↔ FRONTEND_RULES.md / UI-DESIGN.md (only for design-bearing paths)
if touched '^frontend/src/(components/|pages/|index\.css)'; then
  touched '^docs/(UI-DESIGN|FRONTEND_RULES)\.md|^\.claude/agents/frontend\.md' || \
    warnings+=("frontend components/pages/theme changed; check whether docs/UI-DESIGN.md (layout, tokens, state colors) still matches.")
fi

# Deploy artifacts ↔ DEPLOYMENT.md
if touched '^deployment/|^scripts/'; then
  touched '^docs/DEPLOYMENT\.md' || \
    warnings+=("deployment/ or scripts/ changed; review docs/DEPLOYMENT.md (the ops playbook must stay copy-paste exact).")
fi

# Phase plans ↔ ROADMAP status
if touched '^docs/plans/'; then
  touched '^docs/ROADMAP\.md' || \
    warnings+=("a phase plan changed; check that docs/ROADMAP.md status/amendments are current.")
fi

if [ ${#warnings[@]} -gt 0 ]; then
  echo "## Doc-drift check"
  echo
  echo "Unpushed changes touch code whose behavior is described in tracked docs. Update the doc(s) in the same set of commits, or explicitly flag in the response that the doc is now stale."
  echo
  printf -- '- %s\n' "${warnings[@]}"
fi

exit 0
