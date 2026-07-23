# Phase 10 — Production Readiness

**Status:** 🔶 In progress — the control plane went live early (2026-06-11,
user-approved) and a CI gate landed in 0.44.0; audit read-path, telemetry,
structured JSON logs, daily/pre-migration local backups, dependency gates,
and MariaDB integration CI are in place. 0.66.0 added Prometheus `/metrics`
(local-scrape core gauge set), a bounded task re-dispatch ceiling with
controller-side abandonment, and a one-generation rollback path
(`scripts/rollback.sh`). Remaining: load/restart verification, production
backup-timer observation, and the final runbook/hardening acceptance pass
(Phase 9 feeds this).

## Goal

Foundry runs as a supervised production service on this host at
`https://foundry.cloudcraft.ro`, with observability, backups, and runbooks.

## Deliverables

- Nginx vhost installed and live (Cloudflare real-IP, long-poll timeouts,
  rate limits — `../DEPLOYMENT.md`); TLS verified end to end through the
  Cloudflare proxy
- `foundry-controller.service` + `/srv/foundry` layout live; deploy flow in
  `../DEPLOYMENT.md` exercised and exact
- MySQL backup automation (daily + pre-migration, keep 10) — **implemented
  0.51.0**; production credential provisioning/timer observation remains an
  operator acceptance step
- Prometheus `/metrics` exposed locally with the core metric set (servers
  online, slots by state, deployments by state, task queue depth, GitLab
  API health) — **implemented 0.66.0** (`routes/prometheus.rs`; nginx keeps
  the path external-404); journald JSON logging verified
- Graceful shutdown/restart verified under load (in-flight tasks safe)
- Operator runbook complete: enrollment, rotation, instance onboarding,
  common failures (`../DEPLOYMENT.md` § Runtime Truth)
- Version surfacing: app version visible in UI sidebar; release/versioning
  flow documented

## Acceptance

- All 10 success criteria (`../ROADMAP.md`) demonstrated on production at
  `https://foundry.cloudcraft.ro` with a real GitLab instance and a real
  GPU server; v1 tagged
