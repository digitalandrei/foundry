# Phase 4 — Agent Enrollment

**Status:** ✅ Done — GitLab-agent-style enrollment per operator request;
real GPU servers enrolled (protv-ai fleet), with fleet auto-enrollment +
reusable fleet keys added in 0.42.0/0.43.0. **Deferred:** the
agent-credential rotation endpoint (confirm-then-switch) is tracked under
Phase 9 (security hardening).

Delivered: named-server creation in the UI minting the one-time
72h token + full registration command; `POST /agent/enroll` (single
use, credential replace on re-enroll) + `POST /agent/heartbeat`
(ONLINE; 30s sweeper → OFFLINE after 90s quiet); agent-auth extractor
(constant-time, per-server scope); `foundry-agent --register`
(self-install, system user + groups, config 0600, systemd unit,
enable --now) + transition-logged heartbeat loop; Servers page with
live status and token re-minting; binary published at
/downloads/foundry-agent. Smoke-tested end-to-end locally.

## Goal

GPU servers join the fleet via single-use tokens; agents authenticate every
request thereafter. Implements `../ARCHITECTURE.md` § Server Enrollment and
the auth parts of `../SECURITY.md`.

## Deliverables

- Enrollment tokens: admin generation (`POST /api/enrollment-tokens`),
  hashed storage, expiry, single use
- `POST /agent/enroll` → server + agent identity issuance;
  `POST /agent/heartbeat` → ONLINE/OFFLINE tracking
- Agent: `foundry-agent enroll` command, config persistence at
  `/etc/foundry-agent/config.toml` (root-only), heartbeat loop,
  authenticated client
- Token rotation: `POST /api/servers/{id}/rotate-token` + agent-side
  confirm-then-switch
- Servers UI: token generation, server list with health
- systemd unit + install script in `deployment/agent/`

## Test Host

A separate host (Docker installed, **read-only expectations** — observe
only, no container mutations) is available for real-world agent testing;
ask for connection details when starting this phase
(`../ROADMAP.md` § Amendments).

## Acceptance

- A real Ubuntu 24.04 box enrolls with one command and shows ONLINE in the
  UI; restarting the agent reuses the stored identity; rotation invalidates
  the old credential; everything audited
