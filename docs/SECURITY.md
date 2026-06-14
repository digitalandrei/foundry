# Foundry Security Posture

Controls and invariants. Every item here is a hard requirement, not a
suggestion. Review this document for any change touching auth, the agent
protocol, tokens, or deployment execution.

## Principles

- **HTTPS everywhere** — all controller traffic terminates TLS (Nginx +
  Cloudflare in front; see `DEPLOYMENT.md`). Agents verify TLS; no plaintext
  fallback.
- **Least privilege** — users get exactly their GitLab permissions; agents
  can only act on their own server; the controller holds no GitLab admin
  credentials.
- **No remote Docker socket** — the Docker Engine API is only ever accessed
  by the local agent over the local socket.
- **No SSH orchestration** — Foundry never SSHes anywhere.
- **Pull-only data plane** — no inbound connections to GPU servers
  (see `ARCHITECTURE.md`).

## Identity & Sessions

- User identity comes from GitLab OAuth (per onboarded instance) with
  PKCE + CSRF state; the cross-redirect state lives in an encrypted
  10-minute cookie.
- Foundry sessions: HttpOnly/Secure/SameSite=Lax cookie holding a
  256-bit random token; the server stores only its SHA-256
  (`sessions.token_hash`), 7-day TTL, hourly sweeper; logout deletes
  the row.
- GitLab access/refresh tokens and OAuth client secrets are encrypted
  at rest (AES-256-GCM; key = `FOUNDRY_ENCRYPTION_KEY`, base64 32
  bytes). Every controller process sharing the DB must share the key;
  rotation requires re-encrypting stored secrets. Tokens are used
  server-side only.
- The `is_admin` flag governs only Foundry-operational actions (instance
  onboarding, enrollment tokens, token rotation) — never project/registry
  access. Bootstrap admins via `FOUNDRY_ADMIN_EMAILS` (granted at
  login, never auto-revoked).
- **Local operator accounts** (amendment 2026-06-11): username +
  argon2id-hashed password (`local_credentials`), created only via the
  host CLI (`foundry-controller admin add`), always `is_admin`. They
  carry no GitLab identity, so they can administer Foundry but can
  never see projects/registries or deploy — GitLab remains the sole
  authorization source for GitLab resources. Login failures are
  uniform 401s; min password length 12; passwords rotate via
  `admin set-password`.

## Agent Authentication

- Enrollment: single-use, 72h-expiring tokens bound to a named server,
  admin-generated, hash-stored; re-minting revokes unused older tokens.
- Permanent identity: agent id + secret issued at enrollment
  (`X-Foundry-Agent-Id` + `Authorization: Bearer`); secret stored
  hashed (`server_agents.token_hash`), verified constant-time; sent
  over HTTPS on every request.
- Re-enrollment (fresh token + `--register --force`) **replaces** the
  credential — the old one stops working immediately.
- **Token rotation** beyond re-enrollment (admin-triggered confirm-then-
  switch) is planned within Phase 4; not yet implemented.
- An agent's credential authorizes actions only for its own `server_id`.

## Registry Credentials

- Pull credentials are short-lived, scoped to a single repository pull, and
  delivered inside the task payload — never persisted on GPU servers, never
  logged (see `GITLAB-INTEGRATION.md`).
- Exposed-port discovery reuses the same scoped pull token (read-only
  registry fetch of manifest + config blob); failures degrade to an
  empty list, never leak registry errors to other users.

## App Publishing (agent nginx privilege)

The agent writes per-deployment vhosts under `/etc/nginx/foundry-apps/`
(it owns that directory and nothing else under `/etc/nginx`) and
reloads nginx through `/etc/sudoers.d/foundry-agent`, which allows
exactly two commands: `/usr/sbin/nginx -t` and `/usr/sbin/nginx -s
reload` — no shell, no other arguments. Defense in depth around it:

- Hostnames and deployment ids are charset-validated agent-side before
  they reach a conf file (no injection via a compromised controller).
- A failed `nginx -t` rolls the just-written file back, so a bad vhost
  cannot brick reloads for the rest of the server.
- The systemd unit trades two hardening knobs for this feature, by
  design: `NoNewPrivileges` is off (sudo needs the setuid transition)
  and `ProtectSystem` is `full` rather than `strict` (`nginx -t` writes
  logs/temp under `/var` inside the unit's namespace).
  `ReadWritePaths` stays limited to `/etc/foundry-agent` and
  `/etc/nginx/foundry-apps`.
- The wildcard TLS certificate + key are **operator-placed** at
  `/etc/foundry-agent/tls/` on each GPU server. Private keys never
  transit the controller, the database, or the task queue.

## Container shell

The interactive shell (0.22.0, docs/ARCHITECTURE.md § Container shell) is
a real `docker exec` into a running container — treat it as a privileged
capability and keep its controls tight:

- **Authorization is owner/admin**, the same gate as stop/remove; the
  browser WebSocket carries the session cookie and is rejected before the
  upgrade otherwise. The deployment must be RUNNING.
- **Audited**: opening a shell writes a `SHELL_OPENED` audit row (actor,
  deployment, IP).
- **Pull-only preserved**: the controller never dials the agent; the
  agent dials back over an outbound WSS authenticated with its own agent
  credential, and the controller checks the session belongs to that
  agent's `server_id`. No SSH, no inbound port, no remote Docker socket.
- **Scope**: only `foundry.managed=true` containers can be targeted (the
  agent resolves the container by deployment-id label). The exec runs as
  the container's own user — typically root *inside the container*, the
  same blast radius as the workload already has; it is not host root.
- **No persistence**: sessions live only as an in-memory socket pair; a
  controller restart drops them. Bytes are bridged, never logged.
- Residual risk accepted: a shell is as powerful as the container. The
  mitigations are the owner/admin gate + audit trail; tighten by limiting
  who can deploy (deploy rights already come from GitLab membership).

## Audit Logging

- `audit_logs` and `deployment_events` are append-only. Every login,
  onboarding, enrollment, deployment action, replacement, rotation, and
  settings change is recorded with actor, subject, detail, IP, timestamp.
- Audit rows are never updated or deleted by application code.
- Read access (`GET /api/audit`): an admin sees every row; a non-admin is
  scoped to rows they are the actor of (`actor_id = self`). Newest-first,
  cursor-paginated, with an optional exact-match `action` filter.

## Input & Secrets Hygiene

- Validate all external input at API boundaries (extractors + `shared`
  validation). Validate agent uploads too — an agent is authenticated, not
  trusted blindly (bounds-check inventory sizes, log chunk sizes).
- No secrets in source, in logs, or in error messages. `deployment_env`
  values marked secret are encrypted at rest and masked in the UI and logs.
- Containers run with only the devices/ports/volumes the deployment declares;
  no `--privileged` in v1.
- **Deployment visibility vs control** (decision 2026-06-12): the
  deployments list and the dashboard slot grid are **org-visible** to
  every authenticated user — Foundry is an ops dashboard and fleet
  transparency is the point. *Control* (stop/restart/remove/replace)
  is owner-or-admin only, as are volume deletion and volume listing
  (own volumes; admins see all). Image access remains governed by
  GitLab permissions at deploy time.

## Network Posture (this host)

- Controller binds localhost; Nginx is the only public listener
  (see `DEPLOYMENT.md` for Cloudflare specifics, real-IP restoration, and
  rate limiting on `/auth` + `/agent/enroll`).
- `/metrics` is not publicly exposed.
