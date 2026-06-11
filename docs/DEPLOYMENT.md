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

## Controller Host Layout (planned; updated as Phase 10 lands)

| Path | Purpose |
|---|---|
| `/opt/foundry` | Source tree (this repo) |
| `/srv/foundry/foundry-controller` | Deployed controller binary |
| `/srv/foundry/.env` | Controller environment (DB URL, session key, bind addr) |
| `/etc/systemd/system/foundry-controller.service` | systemd unit (in `deployment/`) |
| `/etc/nginx/sites-available/foundry.cloudcraft.ro` | Nginx vhost (in `deployment/`) |

Controller binds `127.0.0.1` only; Nginx is the sole public listener.

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

## Controller Deploy Flow (once Phases 1–2 exist)

```bash
cd /opt/foundry
cargo build --release -p foundry-controller
sudo install -m 755 target/release/foundry-controller /srv/foundry/foundry-controller
sudo systemctl restart foundry-controller
systemctl is-active foundry-controller
curl -fsS http://127.0.0.1:<bind-port>/health
```

Frontend: built with `npm run build` in `frontend/`; serving model (embedded
via rust-embed vs Nginx static root) is decided in Phase 8 — record the
decision here when made.

**Always finish the deploy** — a change is done when it is running on this
host and verified via `/health`, not when it compiles.

## GPU Server Runbook (Ubuntu 24.04)

Prerequisites: NVIDIA driver, Docker Engine, NVIDIA Container Toolkit, MIG
geometry pre-created if desired (`GPU-MIG.md`).

Enrollment:

1. Admin: generate enrollment token in UI (Settings → Servers).
2. On the GPU server: install `foundry-agent` (package/script from
   `deployment/agent/`), then run
   `foundry-agent enroll --controller https://foundry.cloudcraft.ro --token <token>`.
3. Agent stores identity at `/etc/foundry-agent/config.toml` (root-only
   perms) and starts via systemd unit `foundry-agent.service`.
4. Verify: server appears ONLINE in UI with full GPU/MIG inventory.

Agent ops: `systemctl status foundry-agent`,
`journalctl -u foundry-agent -f`. The agent needs only outbound 443.

## Observability

- `GET /health` — liveness; `GET /metrics` — Prometheus (localhost).
- Structured JSON logs in journald for both services.

## Runtime Truth

For troubleshooting, check current truth before reading code:
`/health`, `/metrics`, journald for controller/agent, MySQL state, and the
audit log — not docs alone.
