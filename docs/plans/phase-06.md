# Phase 6 — Deployments (Lifecycle & Replacement)

**Status:** ✅ Done — full lifecycle (deploy/stop/restart/remove),
replacement chain, TCP/UDP port publishing per the design below, persistent
volumes (originally per-user paths; superseded by 0.54.0 project visibility +
slot/server placement with opaque `/storage/containers/volumes/<uuid>` paths,
REMOVE_VOLUME
task), pull-token mint at dispatch (variant 1 with variant-2 fallback),
container-crash reconcile, dnd-kit drag-drop with the per-port-kind dialog —
all live on real GPU servers. The original "Remaining" items shipped:
per-server HTTP/S publishing (0.8.0+) and real GPU deploys, plus later
additions — interactive container shell (0.22.0), GPU groups + multi-use
slots (0.35.0), adopt & control of external containers (0.42.0). Container
logs are Phase 7.

## Networking & Port Publishing (designed 2026-06-12, operator + assistant)

When a user drags an image onto a slot, the deployment dialog collects
the container's ports — **a container may expose any number of ports**;
the dialog is a repeatable row list and every port gets its own kind,
its own allocation, and its own publishing (mixing kinds in one
deployment is normal: e.g. one HTTP UI + one raw TCP gRPC port).
Publishing differs by kind:

| Kind | Meaning | Publishing |
|---|---|---|
| `HTTP` | plain HTTP app | central Nginx `proxy_pass` → `http://<server-ip>:<allocated-port>`, served at `https://<slug>.<apps-domain>` behind Cloudflare |
| `HTTPS` | app terminates its own TLS | same, with `proxy_ssl` upstream (self-signed upstream allowed: `proxy_ssl_verify off`) |
| `TCP` / `UDP` | raw traffic | direct host port mapping on the GPU server's IP (no proxy; UDP is never proxied) |

Port sources: user-entered in the dialog; prefilled when known —
(a) the image's `ExposedPorts` fetched from the registry config blob
when cheap, (b) post-pull the agent inspects the image and reports
actual exposed ports; mismatches surface as a warning on the
deployment, not a failure.

**Conditions / invariants (the "important conditions"):**

1. **No overlap** — host ports are allocated by the controller from a
   per-server pool (default `20000–29999`, configurable per server),
   uniqueness enforced in one transaction against every *active*
   deployment (PENDING…RUNNING, STOPPING) on that server. Ports are
   freed only when the deployment reaches REMOVED/REPLACED or its
   FAILED remnants are cleaned.
2. User-requested fixed ports are allowed for TCP/UDP only, must lie
   in the pool, be free, and never `<1024`; a per-server **reserved
   list** (22, 443, anything the operator names) is always excluded.
3. **Hostname uniqueness** — HTTP(S) deployments get a DNS-safe slug
   (`<deployment-name>`), unique across the apps domain; collision =
   validation error before anything is scheduled.
4. Proxy config exists **only while RUNNING** — created on the
   transition into RUNNING, removed on STOPPING/REPLACED/FAILED; a
   stopped app returns the Foundry 502 page, not a stale upstream.
5. WebSocket upgrade always on; generated vhosts use a 2 GiB
   `client_max_body_size` and 300 s read/send timeouts — large media uploads
   and long inference calls are normal for these apps.
6. One container per port claim — no sharing of an allocated host port
   between deployments, ever; container-to-container traffic stays on
   the Docker bridge and is out of scope for v1.
7. HTTP(S) upstream ports bind `0.0.0.0` on the GPU server (the
   controller host must reach them over the LAN) but are *not*
   advertised to users — the canonical URL is the proxied hostname;
   direct TCP/UDP ports bind the server's public/LAN IP and ARE the
   canonical endpoint.
8. Audit: port allocations and hostname claims are part of the
   deployment audit detail.

**Open items — resolved in 0.8.0 (HTTP/S publishing shipped):**
- Apps wildcard domain: **`*.ai.protv.ro`** (operator choice), enabled
  by `FOUNDRY_APPS_DOMAIN` on the controller.
- The proxy lives **on each GPU server, managed by the agent** — not a
  central nginx on the controller host. Operator wires DNS straight to
  the GPU servers and places the reusable wildcard cert at
  `/etc/foundry-agent/tls/`; no Cloudflare/DNS integration in Foundry.
  The agent writes `/etc/nginx/foundry-apps/<deployment_id>.conf` and
  reloads through a **narrow sudoers entry** (`nginx -t`/`-s reload`),
  set up by `foundry-agent --setup-apps`.
- Deviations from the conditions above, accepted with the per-server
  model: hostnames come from the **container name slug**
  (`<name>.ai.protv.ro`, multi-port `<name>-<port>...`) with global
  uniqueness checked at create (condition 3 holds); the vhost lives
  for the **deployment's whole life** (written at deploy, removed with
  the container) rather than only-while-RUNNING — a stopped app 502s
  because its upstream is down, never a stale upstream (condition 4's
  intent holds); upstreams bind loopback-adjacent on the same host, so
  condition 7's LAN-reachability concern disappears. OCI application metadata
  now controls primary-port selection, health path, body-size, and timeout
  within bounded defaults (2 GiB / 300 s when omitted).
- Port discovery (a) shipped: `GET /api/registry/tags/{id}/exposed-
  ports` reads the image config blob and pre-fills the dialog;
  post-pull verification (b) deferred.

## Goal

The core feature: deploy a registry image to a slot, full lifecycle, and the
replacement workflow. Implements `../ARCHITECTURE.md` § Deployment Lifecycle
and § Agent Tasks.

## Deliverables

- Task queue: `agent_tasks`/`agent_task_results`, `GET /agent/tasks/next`
  (long-poll), `POST /agent/tasks/result`, re-dispatch on timeout
- Deployment state machine in `shared` + single transition function
  (state + event + audit in one transaction, `../RUST_RULES.md` § State
  Machines)
- `POST /api/deployments` (validation: slot FREE, user may pull the tag),
  stop/restart/delete endpoints
- Replacement: `POST /api/deployments/{id}/replace` — prepare/pull the immutable
  successor while the predecessor serves; quiesce and retain the predecessor;
  health-check and publish the successor; remove the old container and mark it
  `REPLACED` only after success. Restore the retained predecessor on failure.
- Agent executors: DEPLOY/STOP/RESTART/REMOVE_CONTAINER via bollard —
  labels, GPU device requests by UUID, ports/env/volumes, short-lived pull
  credentials (`../GITLAB-INTEGRATION.md` § Image Pulls)
- UI: dnd-kit drag from sidebar card to slot chip, deployment config dialog
  (RHF+zod), replacement confirmation dialog, Running Deployments table
  with live state

## Acceptance

- Drag-deploy of a real image onto a real MIG slot reaches RUNNING with GPU
  visible inside the container; replace works with confirmation; every
  transition has a `deployment_events` + audit row; idempotency tests pass
