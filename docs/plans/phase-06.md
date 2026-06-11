# Phase 6 — Deployments (Lifecycle & Replacement)

**Status:** Not started · refine this plan right before starting.

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
5. WebSocket upgrade always on; `client_max_body_size` and
   read-timeout configurable per deployment (defaults 100 MB / 300 s —
   model uploads and long inference calls are the norm here).
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

**Open items to settle when implementation starts:**
- the apps wildcard domain (e.g. `*.apps.cloudcraft.ro`, Cloudflare-
  proxied → this host) — operator to choose;
- mechanism for the controller (runs as `foundry`) to write
  `/etc/nginx/foundry-apps/*.conf` and reload nginx — narrow sudoers
  entry vs. a root-owned path/systemd unit watcher;
- whether LAN-only deployments (no Cloudflare) need the proxy on the
  GPU server itself instead — deferred until a real case appears.

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
- Replacement: `POST /api/deployments/{id}/replace` —
  stop old → remove old → pull new → start new; old ends `REPLACED`
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
