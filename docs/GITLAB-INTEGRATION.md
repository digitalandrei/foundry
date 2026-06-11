# GitLab Integration

How Foundry authenticates users, resolves permissions, and pulls container
images across one or more GitLab instances. GitLab is the source of truth —
Foundry never duplicates permissions locally.

## Multi-Instance Model

> Amendment to the original spec (2026-06-11).

- Instances are onboarded into `gitlab_instances` two ways: the
  **Settings UI** (admin) or the **bootstrap CLI** on the controller
  host — `foundry-controller instance add --name … --base-url …
  --registry-url … --client-id …` with the secret in
  `FOUNDRY_INSTANCE_CLIENT_SECRET` (solves the first-instance
  chicken-and-egg: no admin can log in before an instance exists).
  The controller must be able to reach each instance over HTTPS.
- Each instance needs a GitLab **OAuth application** — any GitLab user
  can create one (Profile → Applications; "Confidential" on; no
  instance admin required):
  - Redirect URI: `https://foundry.cloudcraft.ro/auth/callback`
    (**one fixed URI for all instances** — the pending instance is
    carried in Foundry's encrypted state cookie)
  - Scopes: `openid profile email read_api read_registry`
- Client secrets are AES-256-GCM-encrypted at rest.
- All cached GitLab data (`gitlab_projects`, `registry_repositories`,
  `registry_tags`) and every deployment are keyed to an instance.
- Admin bootstrap: emails listed in `FOUNDRY_ADMIN_EMAILS` get
  `is_admin` granted at login (never auto-revoked).

> **Note:** local operator accounts (`docs/SECURITY.md`) exist for
> portal administration without any GitLab instance — they have no
> GitLab identity and therefore no project/registry visibility; this
> section's authorization model is untouched by them.

> **Decision (2026-06-11): OAuth over self-generated PATs.** Portal-
> triggered OAuth was chosen as the only v1 login/link method — easier
> for users (no token creation/rotation chores), short-lived tokens
> with refresh, and no separate portal-auth system needed. Read-only
> personal access tokens remain a documented **fallback** for a future
> instance where an OAuth app can't be created: `gitlab_accounts` can
> hold a PAT as an access token without refresh, so adding it later is
> one small migration + UI, no redesign.

## OAuth (User Login)

Authorization-code flow against the chosen instance (implemented in
`controller/src/gitlab/oauth.rs` + `auth/routes.rs`):

1. `GET /auth/login/{instance_id}` → redirect to
   `{base_url}/oauth/authorize` with the scopes above, a CSRF `state`,
   and PKCE. The verifier+state+instance travel in the encrypted,
   10-minute `foundry_oauth` cookie — no server-side pending state.
2. `GET /auth/callback` validates state, exchanges the code at
   `{base_url}/oauth/token` (PKCE-verified).
3. Foundry upserts `users` + `gitlab_accounts` (tokens encrypted at
   rest), grants admin per `FOUNDRY_ADMIN_EMAILS`, writes the LOGIN
   audit row, and issues its session cookie. Access tokens are
   refreshed transparently when within 60s of expiry
   (`gitlab/tokens.rs`).

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
