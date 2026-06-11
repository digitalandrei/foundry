---
name: gitlab-api-oauth-registry
description: >
  For the Foundry project at /opt/foundry. GitLab integration mechanics:
  multi-instance OAuth login, REST API v4 usage, container registry
  browsing, and registry pull-token issuance. Use when implementing or
  debugging anything that talks to a GitLab instance.
---

# GitLab API, OAuth & Registry

Design contract: `docs/GITLAB-INTEGRATION.md`. This skill is the
implementation crib sheet. All endpoints are relative to a per-instance
`base_url` from `gitlab_instances` — never hardcode an instance.

## OAuth (per instance)

- Authorization-code + PKCE + `state`:
  `{base}/oauth/authorize?client_id&redirect_uri&response_type=code&scope=openid+profile+email+read_api+read_registry&state&code_challenge`.
- Token exchange: `POST {base}/oauth/token`; refresh with
  `grant_type=refresh_token`. Store both tokens encrypted; refresh
  server-side on expiry; on refresh failure mark the account for re-login.
- Redirect URI is `https://foundry.cloudcraft.ro/auth/callback/{instance}`
  — one OAuth app per onboarded instance.
- User info: `GET {base}/api/v4/user` with the access token.

## REST API v4

- Bearer the **user's** token for permission-sensitive reads — that is the
  authorization mechanism (GitLab decides what they see).
- Always paginate: `per_page=100`, follow `x-next-page` until empty.
- Back off on 429/`RateLimit-Remaining: 0`; treat instance unreachability
  as degraded-cache mode, not an error page
  (`docs/GITLAB-INTEGRATION.md` § Failure Modes).
- Projects: `GET /api/v4/projects?membership=true&simple=true`.
- Registry repositories:
  `GET /api/v4/projects/{id}/registry/repositories`;
  tags: `.../registry/repositories/{repo_id}/tags` (+ `GET .../tags/{name}`
  for digest/size/created).

## Registry Pull Tokens

For agent pulls, exchange for a scoped registry JWT:

```
GET {base}/jwt/auth?service=container_registry&scope=repository:{path}:pull
(authenticated as the deploying user, or instance deploy token)
```

The resulting token goes in the `DEPLOY_CONTAINER` task payload as Docker
registry auth — short-lived, single repository, pull-only. Never persisted
on the GPU server, never logged.

## Testing

Recorded JSON fixtures for API responses; no live GitLab in unit tests.
Token-handling code paths get explicit no-secret-in-logs assertions.
