---
name: https-mtls-agent-transport
description: >
  For the Foundry project at /opt/foundry. Agent↔controller transport
  security: HTTPS posture behind Cloudflare/Nginx, agent credential
  authentication, enrollment tokens, and rotation. Use when implementing
  or reviewing the agent protocol's auth/transport layer.
---

# Agent Transport Security

Controls contract: `docs/SECURITY.md`; topology: `docs/DEPLOYMENT.md`.

## Transport Posture

- Agents speak HTTPS to `https://foundry.cloudcraft.ro` — through
  Cloudflare proxy + Nginx to the localhost-bound controller. TLS is
  always verified (rustls, system roots); no `danger_accept_invalid_certs`,
  no plaintext fallback, ever.
- True mTLS is not possible through Cloudflare's proxy in v1; the
  per-request **agent credential** is the client-auth mechanism. If a
  direct origin hostname is added later for agents, revisit mTLS and
  record the decision in `docs/SECURITY.md`.
- Keep agent uploads (inventory, logs) chunked and bounded — Cloudflare
  body limits apply.

## Credential Model

- **Enrollment token**: single-use, expiring, admin-generated, hash-stored
  (`enrollment_tokens`). Only valid call: `POST /agent/enroll`. Burned on
  use (`used_at`, `used_by_server_id`).
- **Agent identity**: agent id + high-entropy secret issued at enrollment;
  stored hashed in `server_agents.token_hash`; presented on every request
  (Authorization header). Constant-time comparison on verify.
- Scope: a credential authorizes actions for its own `server_id` only —
  task polls, results, inventory, and logs are all filtered by the
  authenticated server, never by client-supplied ids.

## Rotation

- Admin-triggered (`POST /api/servers/{id}/rotate-token`) and periodic.
- Confirm-then-switch: controller issues the new secret as a pending
  credential; the agent confirms by authenticating with it once; the old
  credential is invalidated at that moment (no overlap window beyond
  confirmation, no lockout if the agent missed the message — pending state
  keeps the old one valid until confirm).
- `token_rotated_at` recorded; rotation audited.

## Agent-Side Storage

- Identity lives in `/etc/foundry-agent/config.toml`, root-only (0600).
- Never log the secret; redact Authorization headers in any tracing
  middleware on both ends.
