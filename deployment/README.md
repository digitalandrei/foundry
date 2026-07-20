# Deployment Artifacts

Deployable truth for `docs/DEPLOYMENT.md` — keep both in sync. Controller and
Nginx artifacts are live; backup artifacts are installed by the canonical
deploy flow.

| File | Purpose |
|---|---|
| `systemd/foundry-controller.service` | Controller unit (hardened, EnvironmentFile at `/srv/foundry/.env`, JSON logs) |
| `systemd/foundry-agent.service` | Agent unit (docker group, NVML device access, idempotent-task stop timeout) |
| `nginx/foundry.cloudcraft.ro.conf` | Vhost: Cloudflare real-IP, rate-limited `/auth` + `/agent/enroll`, long-poll `/agent/`, `/metrics` blocked |
| `nginx/snippets/foundry-proxy-headers.conf` | Shared proxy headers (upgrade-ready for SSE/WebSocket logs) |
| `systemd/foundry-backup.{service,timer}` | Daily local MariaDB backup, also invoked before deploy migrations |
| `mysql-backup.cnf.example` | Root-only MariaDB client option-file template (no credentials in argv) |

Prometheus `/metrics` remains a Phase 10 deliverable and is intentionally
blocked by Nginx until implemented.
