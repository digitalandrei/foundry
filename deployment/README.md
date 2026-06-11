# Deployment Artifacts

Deployable truth for `docs/DEPLOYMENT.md` — keep both in sync. All of
these are **drafts until Phase 10 installs them**; nothing here is live
yet.

| File | Purpose |
|---|---|
| `systemd/foundry-controller.service` | Controller unit (hardened, EnvironmentFile at `/srv/foundry/.env`, JSON logs) |
| `systemd/foundry-agent.service` | Agent unit (docker group, NVML device access, idempotent-task stop timeout) |
| `nginx/foundry.cloudcraft.ro.conf` | Vhost: Cloudflare real-IP, rate-limited `/auth` + `/agent/enroll`, long-poll `/agent/`, `/metrics` blocked |
| `nginx/snippets/foundry-proxy-headers.conf` | Shared proxy headers (upgrade-ready for SSE/WebSocket logs) |

Still to come: `agent/` install + enrollment script (Phase 4).
