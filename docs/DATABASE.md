# Foundry Database Schema

MySQL is the sole persistent store (MariaDB 11.4 on this host — see
`DEPLOYMENT.md` § MySQL). Schema is managed exclusively through
sqlx migrations in `migrations/` (forward-only; see the
`mysql-schema-migrations` skill). This document describes every table's
purpose and key columns — it is the human-readable contract; the migrations
are the executable truth. Update both together.

Conventions:

- Primary keys: `id` BINARY(16) (UUIDv7) unless noted.
- Timestamps: `created_at` / `updated_at` DATETIME(6), UTC.
- State columns are VARCHAR enums mirroring the Rust enums in `shared/`
  (single source of truth: `shared`).
- Foreign keys are real constraints; deletes are explicit (no broad cascades
  on audit-bearing tables).

## Identity & GitLab

### `gitlab_instances`
> Amendment: multi-GitLab-instance support (2026-06-11).

Onboarded GitLab instances. One row per instance.
Columns: `id`, `name`, `base_url`, `registry_url`, `oauth_client_id`,
`oauth_client_secret` (encrypted at rest), `enabled`, timestamps.

### `users`
Portal users. A user exists because they logged in via an onboarded instance.
Columns: `id`, `display_name`, `email`, `avatar_url`, `is_admin` (Foundry-side
operator flag for instance/server administration only — never used for
project/registry authorization), `last_login_at`, timestamps.

### `gitlab_accounts`
Links a portal user to their account on a specific GitLab instance.
Columns: `id`, `user_id` FK, `gitlab_instance_id` FK, `gitlab_user_id`,
`username`, `access_token` / `refresh_token` (encrypted at rest),
`token_expires_at`, timestamps. Unique: (`gitlab_instance_id`,
`gitlab_user_id`).

## GitLab Mirror (cache of what GitLab says; refreshed, never authoritative)

### `gitlab_projects`
Projects visible to at least one Foundry user. Columns: `id`,
`gitlab_instance_id` FK, `gitlab_project_id`, `path_with_namespace`, `name`,
`avatar_url`, `last_synced_at`. Unique: (`gitlab_instance_id`,
`gitlab_project_id`).

### `registry_repositories`
Container registry repositories per project. Columns: `id`,
`gitlab_project_id` FK, `gitlab_repository_id`, `path`, `last_synced_at`.

### `registry_tags`
Tags per repository. Columns: `id`, `registry_repository_id` FK, `name`,
`digest`, `size_bytes`, `pushed_at`, `last_synced_at`. A non-positive size
reported by GitLab is stored as unknown; a positive compressed-layer total
read from the registry manifest may fill the cache instead.

Per-user authorization is enforced at request time against GitLab (with
short-lived caching) — these mirror tables exist for browsing speed, not for
access control. Refresh writes are atomic upserts on each table's natural
unique key, so concurrent project browsing and registry polling converge on
the same mirror row.

## Infrastructure

### `servers`
Enrolled GPU servers. `hostname` is **UNIQUE** and nullable (0.42.0 —
the fleet identity: a fleet-enrolling agent auto-creates/re-enrolls its
server by hostname; NULL until enrolled, so name-first servers don't
collide). Columns: `id`, `name`, `hostname`, `ip_address`,
`os_version`, `nvidia_driver_version`, `docker_version`, `status`
(ONLINE/OFFLINE/DEGRADED), `last_heartbeat_at`,
`app_publishing_ready` (BOOL null — HTTP/S publishing readiness from
the latest inventory snapshot; 0.13.0), `nginx_status` (VARCHAR(32)
null — granular reason: READY / NGINX_MISSING / NGINX_OUTDATED /
NGINX_INACTIVE / NOT_CONFIGURED / TLS_MISSING; agent-reported, 0.16.0,
values extended 0.17.0), `docker_ok` (BOOL null — Docker daemon
liveness from the latest snapshot; NULL = unknown/no inventory yet,
false = down → deploys rejected at create; 0.20.0), timestamps.

### `server_agents`
Agent identity and auth per server. Columns: `id`, `server_id` FK,
`agent_version`, `token_hash` (current credential, hashed),
`token_rotated_at`, `enrolled_at`, timestamps.

### `server_containers`
> Added in Phase 5 (amendment): docker-ps visibility.

Observed containers per server — full snapshot per inventory upload
(replace-all). Read-only visibility; Foundry only ever *manages*
containers labeled `foundry.managed=true`. Columns: `id`, `server_id`
FK, `container_id` (short), `name`, `image`, `state`, `status`,
`managed`, `ports` (JSON list of `{container_port, host_port,
protocol}` — a container may expose any number), `gpu_uuids` (JSON
array of GPU/MIG device UUIDs the container is bound to — resolved by
the agent; maps even non-Foundry containers onto a slot), `mounts`
(JSON list of `{source, destination, read_only, mount_type}` — resolved
volume mounts, surfaced so an operator can inspect a container before
adopting it; 0.42.0), `reported_at`.

### `server_metrics`
> Added in the telemetry build (0.5.0).

Rolling telemetry series: `id`, `server_id` FK, `sampled_at`, `sample`
JSON (shape = `foundry_shared::dto::MetricsSample`: host
cpu/mem/disk/net, per-GPU util/mem/temp/power keyed by UUID,
per-container cpu/mem keyed by short id). 30s cadence, 24h retention
(hourly sweeper). The JSON payload is the contract — the schema does
not decompose it.

### `gpus`
Physical GPUs per server. Columns: `id`, `server_id` FK, `gpu_uuid` (NVML),
`model`, `memory_mb`, `mig_enabled`, `last_seen_at`. Unique: `gpu_uuid`.

### `gpu_slots`
Schedulable slots (the unit of deployment). Columns: `id`, `gpu_id` FK,
`slot_type` (FULL_GPU/MIG_SLOT), `mig_uuid` (NVML MIG device UUID, null for
full GPU), `mig_profile` (e.g. `2g.20gb`, null for full GPU), `name`
(display only: card index `3` for a full GPU, `<card>.<slice>` 1-based for a
MIG slot, e.g. `3.1`), `capacity_mb`, `state` (see ARCHITECTURE.md § Slot
states), `max_occupants` (concurrency cap, default 1 = single-use; `>1`
= multi-use soft sharing with no VRAM isolation, capped 1–4 by a CHECK;
operator config, preserved across re-inventory), `last_seen_at`. Unique:
`mig_uuid` where present. For a multi-use slot, live occupancy is the
count of active `deployment_slots` rows, not the binary `state` enum (see
ARCHITECTURE.md § Multi-slot occupancy).

### `gpu_groups`
A named template over whole GPUs on one server: deploying to it runs one
container across all members (1 container : N GPUs — DDP/FSDP/NCCL, or a
model whose VRAM exceeds one card). Columns: `id`, `server_id` FK, `name`,
`max_occupants` (group use-mode: 1 = single-use exclusive, >1 = multi-use
soft sharing of the grouped GPUs, capped 1–4 by a CHECK — same idea as
`gpu_slots.max_occupants`, one level up), `created_by` FK users,
timestamps. Unique: (`server_id`, `name`).

### `gpu_group_members`
Group membership (composite PK `group_id`, `gpu_id`; FK `group_id`
ON DELETE CASCADE; index on `gpu_id` for the reverse "which/how many
groups is this GPU in" lookup). A GPU **may belong to several groups**
(overlap allowed — overlapping groups are mutually exclusive at deploy
time). Members are whole GPUs (FULL slot), MIG-disabled, on one server.

### `deployment_slots`
Authoritative occupancy: which slots a deployment holds (composite PK
`deployment_id`, `gpu_slot_id`; index on `gpu_slot_id`). One row for an
individual deploy, N for a group deploy (one per member GPU). Occupancy
is the count of rows here whose deployment is non-terminal (`state NOT IN
('REMOVED','REPLACED','FAILED')`) — that single count drives both the
multi-use cap check and the group all-members-free rule. Rows are written
at create and never deleted (like `deployment_ports`); terminal
deployments simply stop counting. `deployments.gpu_slot_id` stays as the
denormalised primary (first/only) member for single-slot queries.

### `enrollment_tokens`
Server enrollment tokens. `kind` (0.42.0) discriminates **SERVER** tokens
— single-use, bound to a pre-created server (GitLab-agent style; amendment
2026-06-11) — from **FLEET** keys — reusable within a TTL, `server_id`
NULL, bounded by `max_uses`/`uses`, consumed by `/agent/enroll/fleet`
which auto-creates the server by hostname. Columns: `id`, `token_hash`,
`server_id` FK null (the intended server; NULL for FLEET), `kind`
(SERVER/FLEET), `max_uses` null + `uses` (FLEET budget; NULL = unlimited
within TTL), `created_by` FK users, `expires_at` (SERVER: 72h, re-minting
revokes unused older tokens), `used_at`, `used_by_server_id` FK null
(SERVER consumption record), timestamps.

## Deployments

### `deployments`
One row per deployment attempt onto a slot. Columns: `id`, `gpu_slot_id` FK,
`server_id` FK, `registry_tag_id` FK **null** + `gitlab_instance_id` FK
**null** (both NULL for an *adopted* deployment — no registry origin; 0.42.0)
plus denormalized `image_ref` — full pullable reference at deploy time (the
observed image for an adopted container), `created_by` FK
users, `state` (see ARCHITECTURE.md § Deployment lifecycle), `container_id`
(Docker, once created), `container_name`, `adopted_container_id` null (set
when this wraps an externally-created container — lifecycle/shell/logs
resolve it by this docker id, not by the `foundry.managed` label; 0.42.0),
`active_adoption_key` generated null/string (database-enforced uniqueness for
one non-terminal adoption of a server/container pair),
`mem_limit_mb` null (Docker
`--memory` cap in MB from the deploy slider, clamped 32–256 GB;
NULL = unlimited, the default), `gpu_group_id` FK gpu_groups null
(set for a group deploy — NULL = single-GPU; cleared if the group is
later deleted), `replaced_by_deployment_id` FK null, `error_message`
null, timestamps, `started_at`, `stopped_at`. `gpu_slot_id` is the
denormalised primary member; `deployment_slots` is authoritative for the
full set of GPUs held.
`container_name` is transactionally unique among active deployments on one
server (serialized by the server allocation lock); a replacement is exempt so
it can preserve the URL, and REMOVED/REPLACED or containerless FAILED history
releases the name. This lifecycle-dependent rule is enforced in code rather
than a permanent SQL UNIQUE key.

### `deployment_events`
Append-only state-transition log. Columns: `id`, `deployment_id` FK,
`from_state`, `to_state`, `actor_type` (USER/AGENT/CONTROLLER),
`actor_id` null, `detail` JSON null, `created_at`. Never updated or deleted.

### `deployment_ports`
Port mappings — one row per published port (a container may expose any
number). Columns: `id`, `deployment_id` FK, `container_port`,
`host_port` (controller-allocated from the per-server pool),
`protocol` (tcp/udp), `kind` (HTTP/HTTPS/TCP/UDP — proxy vs direct,
plans/phase-06.md), `hostname` null + KEY (HTTP/S only: the published
app hostname as a per-server subdomain, e.g.
`myapp.protv-ai-04-02.ai.protv.ro` — assigned at create, globally unique
across active deployments except the one a replacement supersedes, so
the URL survives swaps; the agent renders the per-server nginx vhost from
it, ARCHITECTURE.md § App Publishing).

### `deployment_env`
Environment variables. Columns: `id`, `deployment_id` FK, `env_key`,
`env_value` (VARBINARY: encrypted at rest when `is_secret`, UTF-8 bytes
otherwise), `is_secret`. (`env_`-prefixed because `KEY` is a MySQL
reserved word.) Unique: (`deployment_id`, `env_key`).

### `deployment_volumes`
Volume mounts. Columns: `id`, `deployment_id` FK, `server_volume_id`
FK null (the persistent volume backing it; NULLed if the volume is
deleted later), `host_path`, `container_path`, `read_only`,
`purge_on_redeploy` (agent purges the directory before restart/replacement).

### `server_volumes`
> Added in Phase 6 (operator requirement): persistent storage.

Project-scoped local volumes. New paths are opaque
`/storage/containers/volumes/<uuid>` directories (legacy owner/name paths
remain valid). Columns: `id`, `server_id` FK, `gitlab_project_id` FK nullable
only for unattached legacy rows, logical `name`, `visibility`
(PRIVATE/PROJECT), `placement` (SLOT/SERVER), `scope_id` (creator or project
UUID), `placement_id` (slot or server UUID), `gpu_slot_id` FK nullable,
legacy `owner_slug`, `path`, `created_by` FK users, timestamps. Canonical
uniqueness:
(`server_id`, `gitlab_project_id`, `visibility`, `scope_id`, `placement`,
`placement_id`, `name`); path is also unique per server. Clean retains the
row and queues PURGE_VOLUMES; delete removes the row and queues REMOVE_VOLUME.

### `deployment_logs`
> Added in Phase 7 (Logs): captured container stdout+stderr.

Incremental log chunks the agent ships for each **managed** running
container (foreign containers are never read). Columns: `id`,
`deployment_id` FK, `server_id`, `container_id` null, `logged_at`
(newest docker timestamp in the chunk — the retention clock), `content`
MEDIUMTEXT (merged stdout+stderr, `docker --timestamps` lines). Indexed
(`deployment_id`, `logged_at`) and (`logged_at`). Bounded twice: a
half-hourly sweeper drops chunks older than **7 days**, and each append
trims the deployment to its newest N chunks so a log-spamming container
cannot exhaust the controller. Deleted with the deployment when it goes
REMOVED (`lifecycle::transition_deployment`); a STOPPED deployment keeps
its logs.

## Agent Task Queue

### `agent_tasks`
Work queue, polled by agents. Columns: `id`, `server_id` FK,
`deployment_id` FK null, `task_type` (DEPLOY_CONTAINER / STOP_CONTAINER /
RESTART_CONTAINER / REMOVE_CONTAINER / REFRESH_INVENTORY / UPLOAD_LOGS),
`payload` JSON, `state` (QUEUED/DISPATCHED/SUCCEEDED/FAILED/CANCELLED),
`dispatched_at`, `completed_at`, `attempts`, timestamps. Tasks are idempotent;
re-dispatch after agent crash is expected.

### `agent_task_results`
One row per task execution report. Columns: `id`, `agent_task_id` FK,
`success`, `detail` JSON, `logs_excerpt` TEXT null, `reported_at`.

### `local_credentials`
> Added in Phase 3 (amendment): local operator accounts.

Non-GitLab operator logins — portal administration independent of any
GitLab instance (bootstrap, onboarding, ops). One row per local
account, joined to a `users` row created with `is_admin = 1`. Columns:
`user_id` PK/FK, `username` (unique), `password_hash` (argon2id PHC
string), timestamps. Local accounts have no GitLab identity → no
projects/registry/deploy rights; GitLab authorization still comes only
from `gitlab_accounts`. Managed via
`foundry-controller admin add|set-password`.

## Sessions

### `sessions`
> Added in Phase 3 (amendment): server-side session store.

Portal sessions. The browser cookie holds a random 256-bit token; only
its SHA-256 lands here (`token_hash` VARBINARY(32) unique), so a DB
leak yields no usable sessions. Columns: `id`, `token_hash`, `user_id`
FK, `ip_address`, `user_agent`, `expires_at` (7 days), `created_at`.
Expired rows are swept hourly by the controller.

## Audit

### `audit_logs`
Append-only audit trail of every meaningful action (login, instance
onboarding, enrollment, deployment actions, replacements, token rotations,
settings changes). Columns: `id`, `actor_type`, `actor_id` null,
`action`, `subject_type`, `subject_id` null, `detail` JSON, `ip_address`
null, `created_at`. Never updated or deleted.

## Table Count Check

18 spec tables + amendments (`gitlab_instances`, `sessions`,
`local_credentials`, `server_containers`, `server_metrics`,
`server_volumes`, `deployment_logs`, `gpu_groups`, `gpu_group_members`,
`deployment_slots`) = 28 tables total: users, gitlab_accounts,
gitlab_instances, local_credentials, sessions, gitlab_projects,
registry_repositories, registry_tags, servers, server_agents,
server_containers, server_metrics, server_volumes, gpus, gpu_slots,
gpu_groups, gpu_group_members, deployments, deployment_slots,
deployment_events, deployment_ports, deployment_env, deployment_volumes,
deployment_logs, agent_tasks, agent_task_results, audit_logs,
enrollment_tokens.
