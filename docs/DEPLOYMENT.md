# Deployment & Operations

Production setup for the Foundry control plane **on this host**
(Ubuntu 24.04, `/opt/foundry` working tree), plus the GPU-server runbook.
This is the ops playbook — keep it exact and current; commands here are
expected to be copy-pasteable.

## Topology

```
Internet → Cloudflare (proxied DNS: foundry.cloudcraft.ro)
        → Nginx on this host (TLS termination, rate limiting, real IP)
        → foundry-controller (localhost bind)
        → MySQL (localhost)
GPU servers → HTTPS → foundry.cloudcraft.ro (agents; outbound only)
```

`foundry.cloudcraft.ro` is already configured in Cloudflare DNS (proxied).

## Controller Host Layout (LIVE since Phase 3)

| Path | Purpose |
|---|---|
| `/opt/foundry` | Source tree (this repo) |
| `/srv/foundry/foundry-controller` | Deployed controller binary (service user `foundry`) |
| `/srv/foundry/frontend/` | Built SPA, served statically by Nginx |
| `/srv/foundry/.env` | Controller environment (0600): `DATABASE_URL`, `FOUNDRY_ENCRYPTION_KEY` (same key as dev — same DB), `FOUNDRY_ADMIN_EMAILS`, `FOUNDRY_PUBLIC_URL` |
| `/etc/systemd/system/foundry-controller.service` | systemd unit (source: `deployment/systemd/`) |
| `/etc/nginx/sites-available/foundry.cloudcraft.ro` | Nginx vhost (source: `deployment/nginx/`) |
| `/etc/nginx/snippets/foundry-realip.conf`, `foundry-proxy-headers.conf` | CF real-IP (server-scope) + shared proxy headers |
| `/etc/nginx/ssl/foundry.cloudcraft.ro/` | Self-signed origin cert (10y) — host pattern, Cloudflare **Full** mode |

**Serving model (decided Phase 3, supersedes the Phase 8 decision
point): Nginx serves the SPA statically; the controller is API-only.**
No rust-embed — frontend ships by copying `dist/`, no Rust rebuild.

Controller binds `127.0.0.1:8400` (override `FOUNDRY_BIND`); Nginx is
the sole public listener. Other controller env:
`FOUNDRY_DB_MAX_CONNECTIONS` (default 10), `FOUNDRY_LOG_FORMAT=json`
for journald, `RUST_LOG` filter, `FOUNDRY_APPS_DOMAIN=ai.protv.ro`
(enables HTTP/S app publishing — unset rejects HTTP/S port kinds).

**Migrations run automatically at controller startup** (embedded via
`sqlx::migrate!`); a deployed binary is always schema-complete. Manual
application is possible with sqlx-cli (`sqlx migrate run` reading
`DATABASE_URL`) but not required.

## Nginx + Cloudflare Notes

The vhost (template in `deployment/nginx/`) must:

- Terminate TLS (Let's Encrypt or Cloudflare origin certificate; Cloudflare
  proxy mode in front either way).
- Restore client IPs: `set_real_ip_from` for Cloudflare ranges +
  `real_ip_header CF-Connecting-IP` — audit logs record real client IPs.
- Proxy `/`, `/api/`, `/auth/`, `/agent/` to the controller;
  `proxy_http_version 1.1` with upgrade headers (SSE/WebSocket-ready for
  live logs).
- Long-poll friendliness: `proxy_read_timeout` ≥ 90s on `/agent/tasks/next`
  and on log streaming routes.
- Rate-limit `/auth/` and `/agent/enroll`.
- Do not expose `/metrics`.

Agent traffic also flows through Cloudflare unless a direct origin hostname
is later added — keep agent request bodies (log/inventory uploads) chunked
and modest to stay within proxy limits.

## MySQL (MariaDB)

- This host runs **MariaDB 11.4** (MySQL-compatible; the `mysql` client is
  a deprecated alias for `mariadb`). sqlx's MySQL driver targets it.
- Database `foundry` (utf8mb4/utf8mb4_unicode_ci) with dedicated user
  `foundry@localhost`, granted `ALL` on `foundry.*` **only** — no access
  to other databases on this shared server (provisioned 2026-06-11).
- Credentials live in `/opt/foundry/.env` (gitignored, mode 600,
  `DATABASE_URL`); the production copy moves to `/srv/foundry/.env` in
  Phase 10.
- Schema applied exclusively via `sqlx migrate run` from `migrations/`.
- Backups: daily dump + pre-migration dump before any destructive migration,
  keep last 10 (same discipline as other projects on this host).

## Deploy Flow (live)

**One command — `scripts/deploy.sh`** (run from the repo root, needs
sudo). It is the canonical path: it gates on Cargo.toml ↔
frontend/package.json version parity, **rebuilds both the
controller/agent and the frontend** (a version bump never ships a stale
GUI), **replaces the SPA tree wholesale** (no leftover hashed bundles
accumulating under `/srv/foundry/frontend/assets`), installs the
controller + agent binaries, restarts the service, and verifies
`/health` reports the new version.

```bash
cd /opt/foundry
# bump the workspace + frontend version together first (operator rule)
./scripts/deploy.sh
curl -fsS https://foundry.cloudcraft.ro/health   # end-to-end through Cloudflare
```

Equivalent manual steps (only when debugging the script) — note the
**clean** before copying the SPA, which the old flow lacked and which
let stale bundles pile up:

```bash
cargo build --release -p foundry-controller -p foundry-agent
(cd frontend && npm run build)
sudo find /srv/foundry/frontend -mindepth 1 -maxdepth 1 -exec \rm -rf {} +
sudo \cp -rf frontend/dist/. /srv/foundry/frontend/
sudo chown -R foundry:foundry /srv/foundry/frontend
sudo install -m 755 target/release/foundry-controller /srv/foundry/foundry-controller
sudo install -m 755 target/release/foundry-agent /srv/foundry/downloads/foundry-agent
sudo systemctl restart foundry-controller
curl -fsS http://127.0.0.1:8400/health   # {"status":"ok","version":"X.Y.Z","database":"up"}
```

Nginx config changes: edit in `deployment/nginx/`, then
`sudo install -m 644 deployment/nginx/foundry.cloudcraft.ro.conf /etc/nginx/sites-available/foundry.cloudcraft.ro && sudo nginx -t && sudo systemctl reload nginx`.
(Host note: nginx 1.24 — use the `listen … ssl http2;` form, and
real-IP directives live in the **server-scope** snippet because other
vhosts own the http-scope `real_ip_header`.)

Dev loop on this host: controller `FOUNDRY_BIND=127.0.0.1:8401
FOUNDRY_PUBLIC_URL=http://localhost:5173 cargo run -p
foundry-controller` (8400 is the production service), frontend
`cd frontend && npm run dev` (vite proxies `/api` `/auth` `/health` to
8401 — adjust the proxy target in `vite.config.ts` if you change the
port). Agent against a dev controller:
`FOUNDRY_AGENT_CONFIG=/tmp/agent.toml cargo run -p foundry-agent`.

**Always finish the deploy** — a change is done when it is running on this
host and verified via `/health`, not when it compiles.

## GPU Server Runbook (Ubuntu 24.04+, glibc build)

Prerequisites: NVIDIA driver, Docker Engine, NVIDIA Container Toolkit, MIG
geometry pre-created if desired (`GPU-MIG.md`).

Enrollment (GitLab-agent style):

1. Admin: **Servers → Add server** in the UI — names the server and
   shows the one-time registration command (token: single-use, 72h;
   "New token" re-mints and revokes unused older ones).
2. On the GPU server:

   ```bash
   curl -fsSLo /usr/local/bin/foundry-agent \
     https://foundry.cloudcraft.ro/downloads/foundry-agent
   chmod +x /usr/local/bin/foundry-agent
   sudo foundry-agent --register --url https://foundry.cloudcraft.ro --token <token>
   ```

   `--register` enrolls, installs the binary to `/usr/local/bin` if run
   from elsewhere, creates the `foundry-agent` system user (joining
   docker/video/render groups where present), writes
   `/etc/foundry-agent/config.toml` (0600), runs the app-publishing
   setup (below), writes the systemd unit, and
   `systemctl enable --now`s it. `--force` re-enrolls an
   already-registered server with a fresh token (replaces the
   credential).
3. Verify: server flips ONLINE in the UI within ~15 s (heartbeat).

**Agent upgrade** (and app-publishing setup/repair) on an enrolled
server — download the new binary as in step 2, then:

```bash
sudo ./foundry-agent --setup-apps
```

Installs the binary, ensures `/etc/nginx/foundry-apps/` (+ the conf.d
include with the websocket map), `/etc/foundry-agent/tls/`, the
sudoers rule scoped to `nginx -t`/`-s reload` (SECURITY.md § App
Publishing), prepares `/storage/containers` for persistent volumes
(service-user-owned; listed in the unit's ReadWritePaths), rewrites
the systemd unit, and restarts the service.

**HTTP/S app publishing prerequisites per GPU server** (operator,
once, **per server**): install nginx, point wildcard DNS
`*.<server>.ai.protv.ro` (e.g. `*.protv-ai-04-02.ai.protv.ro`) at that
GPU server, and place/renew a wildcard certificate for
`*.<server>.ai.protv.ro` at `/etc/foundry-agent/tls/fullchain.pem` +
`privkey.pem` on it. Ports 80/443 must be reachable. Apps then publish
at `https://<name>.<server>.ai.protv.ro` (multi-port:
`<name>-<container_port>.<server>...`). The per-server subdomain makes
DNS and certs predictable: one wildcard per server covers every app on
it.

Agent ops: `systemctl status|stop|restart foundry-agent`,
`journalctl -u foundry-agent -f` (JSON logs; state-transition lines
only). The agent needs only outbound 443 (+ the registry port at deploy
time, e.g. `g.protv.ro:5050`); inbound 80/443 once it publishes apps.
The published binary lives at `/srv/foundry/downloads/foundry-agent`
(served by the vhost, updated on every controller deploy).

## Observability

- `GET /health` — liveness; `GET /metrics` — Prometheus (localhost).
- Structured JSON logs in journald for both services.

## Runtime Truth

For troubleshooting, check current truth before reading code:
`/health`, `/metrics`, journald for controller/agent, MySQL state, and the
audit log — not docs alone.
