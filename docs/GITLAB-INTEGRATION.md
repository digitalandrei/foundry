# GitLab Integration

How Foundry authenticates users, resolves permissions, and pulls container
images across one or more GitLab instances. GitLab is the source of truth —
Foundry never duplicates permissions locally.

## Multi-Instance Model

> Amendment to the original spec (2026-06-11).

- Instances are onboarded by an admin into `gitlab_instances` (base URL,
  registry URL, per-instance OAuth app credentials). The controller must be
  able to reach each instance over HTTPS.
- Each instance needs a GitLab OAuth application created on that instance
  with redirect URI `https://foundry.cloudcraft.ro/auth/callback/{instance}`
  and scopes below.
- All cached GitLab data (`gitlab_projects`, `registry_repositories`,
  `registry_tags`) and every deployment are keyed to an instance.

## OAuth (User Login)

Authorization-code flow against the chosen instance:

1. `GET /auth/login/{instance}` → redirect to
   `{base_url}/oauth/authorize` with scopes
   `openid profile email read_api read_registry`, a CSRF `state`, and PKCE.
2. Callback exchanges the code at `{base_url}/oauth/token`.
3. Foundry upserts `users` + `gitlab_accounts` (tokens encrypted at rest)
   and issues its own session cookie. GitLab tokens are refreshed via the
   refresh token when expired.

The GitLab access token is used server-side only — never sent to the
browser, never sent to agents (agents receive short-lived pull credentials
per deploy task, see below).

## Authorization Resolution

Permission checks ask GitLab with the **user's** token:

- Project visibility: `GET /api/v4/projects?membership=true&min_access_level=...`
  and per-project lookups.
- A user may deploy an image iff their GitLab account can read that project's
  registry (project visible + `read_registry` works against it).

Responses are cached briefly (minutes) for browsing speed; any
deployment-creating request re-validates against GitLab. Mirror tables are a
cache, not an ACL.

## GitLab API Usage

- REST v4 via `reqwest`; always paginate (`per_page=100`, follow
  `x-next-page`), respect rate limits with backoff.
- Registry browsing:
  `GET /api/v4/projects/{id}/registry/repositories` and
  `.../repositories/{repo_id}/tags` (+ tag detail for digest/size/pushed_at).

## Image Pulls on GPU Servers

Agents must authenticate to the instance's container registry to pull:

- The controller embeds short-lived pull credentials in the
  `DEPLOY_CONTAINER` task payload. Source of credentials: the deploying
  user's token exchanged for a registry token (JWT from
  `{base_url}/jwt/auth?service=container_registry&scope=repository:{path}:pull`),
  or a per-instance deploy token if configured — decided per instance at
  onboarding.
- The agent uses the credential for `docker pull` (Engine API auth header)
  and discards it; credentials are never written to disk on GPU servers.

## Failure Modes to Handle

- Instance unreachable → browsing degrades to cached data clearly marked
  stale; deployments to that instance's images are blocked.
- Token expired and refresh fails → user is prompted to re-login; background
  syncs skip that account.
- Registry tag deleted upstream → deployment validation fails in
  `VALIDATING` with a clear error.
