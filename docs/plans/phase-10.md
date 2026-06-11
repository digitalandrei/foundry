# Phase 10 — Production Readiness

**Status:** Not started · refine this plan right before starting.

## Goal

Foundry runs as a supervised production service on this host at
`https://foundry.cloudcraft.ro`, with observability, backups, and runbooks.

## Deliverables

- Nginx vhost installed and live (Cloudflare real-IP, long-poll timeouts,
  rate limits — `../DEPLOYMENT.md`); TLS verified end to end through the
  Cloudflare proxy
- `foundry-controller.service` + `/srv/foundry` layout live; deploy flow in
  `../DEPLOYMENT.md` exercised and exact
- MySQL backup automation (daily + pre-migration, keep 10)
- Prometheus `/metrics` exposed locally with the core metric set (servers
  online, slots by state, deployments by state, task queue depth, GitLab
  API health); journald JSON logging verified
- Graceful shutdown/restart verified under load (in-flight tasks safe)
- Operator runbook complete: enrollment, rotation, instance onboarding,
  common failures (`../DEPLOYMENT.md` § Runtime Truth)
- Version surfacing: app version visible in UI sidebar; release/versioning
  flow documented

## Acceptance

- All 10 success criteria (`../ROADMAP.md`) demonstrated on production at
  `https://foundry.cloudcraft.ro` with a real GitLab instance and a real
  GPU server; v1 tagged
