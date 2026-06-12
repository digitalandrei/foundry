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
`digest`, `size_bytes`, `pushed_at`, `last_synced_at`.

Per-user authorization is enforced at request time against GitLab (with
short-lived caching) — these mirror tables exist for browsing speed, not for
access control.

## Infrastructure

### `servers`
Enrolled GPU servers. Columns: `id`, `name`, `hostname`, `ip_address`,
`os_version`, `nvidia_driver_version`, `docker_version`, `status`
(ONLINE/OFFLINE/DEGRADED), `last_heartbeat_at`, timestamps.

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
protocol}` — a container may expose any number), `reported_at`.

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
(display, e.g. `0:2`), `capacity_mb`, `state` (see ARCHITECTURE.md § Slot
states), `last_seen_at`. Unique: `mig_uuid` where present.

### `enrollment_tokens`
Single-use server enrollment tokens, bound to a pre-created server
(GitLab-agent style; amendment 2026-06-11). Columns: `id`, `token_hash`,
`server_id` FK (the intended server), `created_by` FK users,
`expires_at` (72h; re-minting revokes unused older tokens), `used_at`,
`used_by_server_id` FK null (consumption record), timestamps.

## Deployments

### `deployments`
One row per deployment attempt onto a slot. Columns: `id`, `gpu_slot_id` FK,
`server_id` FK, `registry_tag_id` FK (plus denormalized `image_ref` — full
pullable reference at deploy time), `gitlab_instance_id` FK, `created_by` FK
users, `state` (see ARCHITECTURE.md § Deployment lifecycle), `container_id`
(Docker, once created), `container_name`, `replaced_by_deployment_id` FK
null, `error_message` null, timestamps, `started_at`, `stopped_at`.

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
deleted later), `host_path`, `container_path`, `read_only`.

### `server_volumes`
> Added in Phase 6 (operator requirement): persistent storage.

Named per-server, per-user volumes at
`/storage/containers/<owner_slug>/<name>`. Created on first use at
deploy; survive container removal; remountable into later containers;
deleted explicitly (REMOVE_VOLUME agent task wipes the directory).
Columns: `id`, `server_id` FK, `name`, `owner_slug`, `path`,
`created_by` FK users, timestamps. Unique: (`server_id`, `created_by`,
`name`) and (`server_id`, `path`).

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
`server_volumes`) = 24 tables total: users, gitlab_accounts,
gitlab_instances, local_credentials, sessions, gitlab_projects,
registry_repositories, registry_tags, servers, server_agents,
server_containers, server_metrics, server_volumes, gpus, gpu_slots,
deployments, deployment_events, deployment_ports, deployment_env,
deployment_volumes, agent_tasks, agent_task_results, audit_logs,
enrollment_tokens.
