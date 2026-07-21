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
- **Fleet keys** (0.42.0): reusable, time-limited, admin-minted keys
  (hash-stored, bounded by an explicit TTL and optional `max_uses`) for
  auto-enrolling a fleet. A presenting agent auto-creates/re-enrolls its
  server by a now-unique hostname; the key never yields more than the
  permanent per-server identity and expires on its own. Keep TTLs short and
  prefer a use budget for one-shot rollouts.
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
  `ReadWritePaths` stays limited to `/etc/foundry-agent`,
  `/etc/nginx/foundry-apps`, `/storage/containers`, and the dedicated
  `/var/log/nginx/foundry-apps` directory required by the enabled features.
- Only `CAP_DAC_OVERRIDE` is ambient in the agent. The bounding set retains
  `CAP_SETUID`, `CAP_SETGID`, `CAP_AUDIT_WRITE`, and
  `CAP_NET_BIND_SERVICE` so the setuid-root sudo child can change identity,
  initialize Ubuntu's audit plugin, and validate configurations that bind
  privileged ports. Those capabilities are not granted ambiently to the
  long-running agent.
- The wildcard TLS certificate + key are **operator-placed** at
  `/etc/foundry-agent/tls/` on each GPU server. Private keys never
  transit the controller, the database, or the task queue.
- Per-app nginx records contain request time, method, normalized URI path,
  status, bytes, and request ID—not headers, cookies, bodies, or query
  strings. Host files rotate for seven days; controller rows have the same
  retention and are removed with the deployment.

### Agent self-upgrade helper

The long-running agent is not root. `UPGRADE_AGENT` only lets it create the
fixed `/etc/foundry-agent/upgrade-request` marker. A root-owned systemd path
unit invokes one fixed `foundry-agent --perform-upgrade` command, which uses
the already-enrolled controller URL, downloads the published binary and its
SHA-256 file over verified HTTPS, checks the digest, atomically renames the
binary, and runs the fixed `--setup-apps` repair. No task payload supplies a
URL, path, command, or arguments. A compromised controller is already in the
agent binary trust boundary; the checksum protects corruption/cache mismatch,
not a malicious controller.

## Container shell

The interactive shell (0.22.0, docs/ARCHITECTURE.md § Container shell) is
a real `docker exec` into a running container — treat it as a privileged
capability and keep its controls tight:

- **Authorization is owner/admin**, the same gate as stop/remove; the
  browser WebSocket carries the session cookie and is rejected before the
  upgrade otherwise. The deployment must be RUNNING or PUBLISH_FAILED (the
  latter is a healthy running container whose nginx route is not live).
- **Audited**: opening a shell writes a `SHELL_OPENED` audit row (actor,
  deployment, IP).
- **Pull-only preserved**: the controller never dials the agent; the
  agent dials back over an outbound WSS authenticated with its own agent
  credential, and the controller checks the session belongs to that
  agent's `server_id`. No SSH, no inbound port, no remote Docker socket.
- **Scope**: Foundry-created containers (resolved by the deployment-id
  label) **and** operator-**adopted** containers (resolved by docker id —
  see § Adopted containers) can be targeted. The exec runs as the
  container's own user — typically root *inside the container*, the same
  blast radius as the workload already has; it is not host root.
- **No persistence**: sessions live only as an in-memory socket pair; a
  controller restart drops them. Bytes are bridged, never logged.
- Residual risk accepted: a shell is as powerful as the container. The
  mitigations are the owner/admin gate + audit trail; tighten by limiting
  who can deploy (deploy rights already come from GitLab membership).

## Persistent-volume file sessions

The Storage browser is host-filesystem access, so its scope is explicit at
every boundary:

- The browser session requires an authenticated Foundry user. Its roots are
  scoped by the logical hierarchy server → shared/slot/group → user-given
  deploy-name project → mount; a deployment-detail session is additionally
  narrowed to the volume IDs attached to that deployment. The deploy name is
  not a GitLab ACL.
- The browser sends a `volume_id` and relative UTF-8 path, never a host path.
  The agent receives a controller-approved root map for its own `server_id`;
  absolute paths, `..`, root deletion, and symlink following are rejected.
- Deploy-time reuse has the same trust boundary. A client may name an existing
  `volume_id` and an absolute **container** destination, but never a host
  source path. The controller locks and resolves the stored root, then accepts
  it only when it is on the selected server and is either the target's exact
  SLOT/GPU-group root or a SERVER root. A crafted request cannot mount another
  server, another slot, or another group merely because the user learned its
  ID. Its redundant request name/placement must match the selected row, while
  its deployment-name project is intentionally allowed to differ. Docker
  receives the controller-selected source as a bind mount; RO/RW is
  deliberately per binding rather than a mutation of the source root.
- Every signed-in operator may browse or explicitly mount a compatible root,
  including a root created under another user-given deployment-name project.
  That is operational sharing, not a GitLab-project ACL. Creator/admin-only
  management still applies to clean, delete, and quota changes; clean/delete
  remain blocked while mounted.
- New physical roots are controller-allocated below the reserved
  `/storage/containers/.foundry/` namespace and end in an immutable volume
  UUID. The controller's `{volume_id,path}` catalog is the accounting and
  authorization authority: the agent measures only cataloged roots for its
  authenticated server, never roots inferred by enumerating the filesystem.
  Retained legacy paths remain cataloged explicitly. Before create, mount,
  measurement, purge, or deletion, the agent validates every existing path
  component with `symlink_metadata`; symlink/non-directory ancestors are
  rejected, and destructive work uses the validated canonical root.
- Session open and every mutation request are audited. Audit detail contains
  operation, volume IDs, paths and upload size—but never file content or
  transfer chunks. Deployment and replacement audits likewise record
  automatic-vs-existing source selection, source volume ID, container
  destination, RO/RW, and purge policy, but never file content or an
  untrusted client host path.
- Container files may be owned by arbitrary numeric UIDs. The agent unit
  receives `CAP_DAC_OVERRIDE`; its intended writable storage root remains
  `/storage/containers` and approved-root checks are the authority
  (`ProtectSystem=full` keeps system trees read-only in the unit mount
  namespace). No listener is added: the agent long-polls and dials outbound
  WSS.
- Text reads/writes are UTF-8-only and capped at 2 MiB; arbitrary files use
  chunked transfer. Upload IDs and partial files support reconnect/resume;
  offsets are agent-authoritative and the final rename is atomic. Configured
  quotas reject browser uploads that would exceed measured usage, but are
  advisory—not filesystem quotas—so a container can still exceed them.
  Shared-volume application locking remains the users' responsibility.

## Adopted containers

Foundry's core invariant is that it only mutates containers it created
(`foundry.managed=true`). Adoption (0.42.0) is a deliberate, bounded
relaxation, not a removal of that rule:

- **Explicit & admin-gated**: a foreign container is only ever acted on
  after an admin adopts it (`POST …/adopt`), creating a deployment row with
  `adopted_container_id`. The agent never touches a foreign container that
  has not been adopted — there is no blind sweep.
- **Resolved by id, not label**: lifecycle/shell/logs target the adopted
  container by its docker id (the agent can't relabel a running container);
  the resolution path is otherwise the same authenticated, pull-only one.
- **Double-confirmed destructive ops**: stopping/deleting an adopted
  container is a type-to-confirm action in the UI (the operator must type
  the container name) — it removes a container Foundry did not create.
- **Audited**: adoption (`CONTAINER_ADOPTED`) and every subsequent
  lifecycle transition are recorded with actor, subject, and detail.

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
- **Deployment visibility vs control** (decision 2026-06-12; placement
  storage amendment 0.63.0): the
  deployments list and the dashboard slot grid are **org-visible** to
  every authenticated user — Foundry is an ops dashboard and fleet
  transparency is the point. Stop/restart/remove remain owner-or-admin.
  Replacement is owner/admin, or another current GitLab member when both
  deployments belong to the same project. Persistent volume reuse and file
  access follow the physical SLOT/SERVER placement and are available to
  signed-in operators; destructive clean/delete/quota remains creator-or-admin
  and is refused while mounted. Project listing, deployment, and replacement
  are resolved live against GitLab; mirror tables are never a storage ACL.

## Network Posture (this host)

- Controller binds localhost; Nginx is the only public listener
  (see `DEPLOYMENT.md` for Cloudflare specifics, real-IP restoration, and
  rate limiting on `/auth` + `/agent/enroll`).
- Prometheus `/metrics` is not implemented yet and remains blocked at nginx;
  it must not be publicly exposed when added.
