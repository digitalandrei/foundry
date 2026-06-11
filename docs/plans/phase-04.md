# Phase 4 — Agent Enrollment

**Status:** Not started · refine this plan right before starting.

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

## Acceptance

- A real Ubuntu 24.04 box enrolls with one command and shows ONLINE in the
  UI; restarting the agent reuses the stored identity; rotation invalidates
  the old credential; everything audited
