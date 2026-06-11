-- Initial Foundry schema: all 19 tables from docs/DATABASE.md.
-- Conventions: BINARY(16) UUIDv7 PKs, DATETIME(6) UTC app-managed
-- timestamps, VARCHAR state columns holding foundry-shared enum strings,
-- named FKs, secrets in VARBINARY (encrypted by the app before INSERT).

-- ─── Identity & GitLab ──────────────────────────────────────────────

CREATE TABLE gitlab_instances (
    id                  BINARY(16)    NOT NULL PRIMARY KEY,
    name                VARCHAR(100)  NOT NULL,
    base_url            VARCHAR(255)  NOT NULL,
    registry_url        VARCHAR(255)  NOT NULL,
    oauth_client_id     VARCHAR(255)  NOT NULL,
    oauth_client_secret VARBINARY(1024) NOT NULL,
    enabled             TINYINT(1)    NOT NULL DEFAULT 1,
    created_at          DATETIME(6)   NOT NULL,
    updated_at          DATETIME(6)   NOT NULL,
    UNIQUE KEY uq_gitlab_instances_name (name)
);

CREATE TABLE users (
    id            BINARY(16)   NOT NULL PRIMARY KEY,
    display_name  VARCHAR(255) NOT NULL,
    email         VARCHAR(255) NULL,
    avatar_url    VARCHAR(512) NULL,
    is_admin      TINYINT(1)   NOT NULL DEFAULT 0,
    last_login_at DATETIME(6)  NULL,
    created_at    DATETIME(6)  NOT NULL,
    updated_at    DATETIME(6)  NOT NULL
);

CREATE TABLE gitlab_accounts (
    id                 BINARY(16)      NOT NULL PRIMARY KEY,
    user_id            BINARY(16)      NOT NULL,
    gitlab_instance_id BINARY(16)      NOT NULL,
    gitlab_user_id     BIGINT          NOT NULL,
    username           VARCHAR(255)    NOT NULL,
    access_token       VARBINARY(4096) NULL,
    refresh_token      VARBINARY(4096) NULL,
    token_expires_at   DATETIME(6)     NULL,
    created_at         DATETIME(6)     NOT NULL,
    updated_at         DATETIME(6)     NOT NULL,
    UNIQUE KEY uq_gitlab_accounts_instance_user (gitlab_instance_id, gitlab_user_id),
    KEY idx_gitlab_accounts_user (user_id),
    CONSTRAINT fk_gitlab_accounts_user
        FOREIGN KEY (user_id) REFERENCES users (id),
    CONSTRAINT fk_gitlab_accounts_instance
        FOREIGN KEY (gitlab_instance_id) REFERENCES gitlab_instances (id)
);

-- ─── GitLab mirror (cache, never an ACL) ────────────────────────────

CREATE TABLE gitlab_projects (
    id                  BINARY(16)   NOT NULL PRIMARY KEY,
    gitlab_instance_id  BINARY(16)   NOT NULL,
    gitlab_project_id   BIGINT       NOT NULL,
    path_with_namespace VARCHAR(512) NOT NULL,
    name                VARCHAR(255) NOT NULL,
    avatar_url          VARCHAR(512) NULL,
    last_synced_at      DATETIME(6)  NULL,
    created_at          DATETIME(6)  NOT NULL,
    updated_at          DATETIME(6)  NOT NULL,
    UNIQUE KEY uq_gitlab_projects_instance_project (gitlab_instance_id, gitlab_project_id),
    CONSTRAINT fk_gitlab_projects_instance
        FOREIGN KEY (gitlab_instance_id) REFERENCES gitlab_instances (id)
);

CREATE TABLE registry_repositories (
    id                   BINARY(16)   NOT NULL PRIMARY KEY,
    gitlab_project_id    BINARY(16)   NOT NULL,
    gitlab_repository_id BIGINT       NOT NULL,
    path                 VARCHAR(512) NOT NULL,
    last_synced_at       DATETIME(6)  NULL,
    created_at           DATETIME(6)  NOT NULL,
    updated_at           DATETIME(6)  NOT NULL,
    UNIQUE KEY uq_registry_repositories_project_repo (gitlab_project_id, gitlab_repository_id),
    CONSTRAINT fk_registry_repositories_project
        FOREIGN KEY (gitlab_project_id) REFERENCES gitlab_projects (id)
);

CREATE TABLE registry_tags (
    id                     BINARY(16)   NOT NULL PRIMARY KEY,
    registry_repository_id BINARY(16)   NOT NULL,
    name                   VARCHAR(255) NOT NULL,
    digest                 VARCHAR(255) NULL,
    size_bytes             BIGINT       NULL,
    pushed_at              DATETIME(6)  NULL,
    last_synced_at         DATETIME(6)  NULL,
    created_at             DATETIME(6)  NOT NULL,
    updated_at             DATETIME(6)  NOT NULL,
    UNIQUE KEY uq_registry_tags_repo_name (registry_repository_id, name),
    CONSTRAINT fk_registry_tags_repository
        FOREIGN KEY (registry_repository_id) REFERENCES registry_repositories (id)
);

-- ─── Infrastructure ─────────────────────────────────────────────────

CREATE TABLE servers (
    id                    BINARY(16)   NOT NULL PRIMARY KEY,
    name                  VARCHAR(255) NOT NULL,
    hostname              VARCHAR(255) NOT NULL,
    ip_address            VARCHAR(45)  NULL,
    os_version            VARCHAR(128) NULL,
    nvidia_driver_version VARCHAR(64)  NULL,
    docker_version        VARCHAR(64)  NULL,
    status                VARCHAR(16)  NOT NULL DEFAULT 'OFFLINE',
    last_heartbeat_at     DATETIME(6)  NULL,
    created_at            DATETIME(6)  NOT NULL,
    updated_at            DATETIME(6)  NOT NULL,
    UNIQUE KEY uq_servers_name (name),
    KEY idx_servers_status (status)
);

CREATE TABLE server_agents (
    id               BINARY(16)    NOT NULL PRIMARY KEY,
    server_id        BINARY(16)    NOT NULL,
    agent_version    VARCHAR(64)   NULL,
    token_hash       VARBINARY(64) NOT NULL,
    token_rotated_at DATETIME(6)   NULL,
    enrolled_at      DATETIME(6)   NOT NULL,
    created_at       DATETIME(6)   NOT NULL,
    updated_at       DATETIME(6)   NOT NULL,
    UNIQUE KEY uq_server_agents_server (server_id),
    CONSTRAINT fk_server_agents_server
        FOREIGN KEY (server_id) REFERENCES servers (id)
);

CREATE TABLE gpus (
    id           BINARY(16)   NOT NULL PRIMARY KEY,
    server_id    BINARY(16)   NOT NULL,
    gpu_uuid     VARCHAR(64)  NOT NULL,
    model        VARCHAR(128) NULL,
    memory_mb    INT UNSIGNED NULL,
    mig_enabled  TINYINT(1)   NOT NULL DEFAULT 0,
    last_seen_at DATETIME(6)  NULL,
    created_at   DATETIME(6)  NOT NULL,
    updated_at   DATETIME(6)  NOT NULL,
    UNIQUE KEY uq_gpus_gpu_uuid (gpu_uuid),
    KEY idx_gpus_server (server_id),
    CONSTRAINT fk_gpus_server
        FOREIGN KEY (server_id) REFERENCES servers (id)
);

CREATE TABLE gpu_slots (
    id           BINARY(16)   NOT NULL PRIMARY KEY,
    gpu_id       BINARY(16)   NOT NULL,
    slot_type    VARCHAR(16)  NOT NULL,
    mig_uuid     VARCHAR(64)  NULL,
    mig_profile  VARCHAR(32)  NULL,
    name         VARCHAR(32)  NOT NULL,
    capacity_mb  INT UNSIGNED NULL,
    state        VARCHAR(16)  NOT NULL DEFAULT 'OFFLINE',
    last_seen_at DATETIME(6)  NULL,
    created_at   DATETIME(6)  NOT NULL,
    updated_at   DATETIME(6)  NOT NULL,
    UNIQUE KEY uq_gpu_slots_mig_uuid (mig_uuid),
    KEY idx_gpu_slots_gpu (gpu_id),
    KEY idx_gpu_slots_state (state),
    CONSTRAINT fk_gpu_slots_gpu
        FOREIGN KEY (gpu_id) REFERENCES gpus (id)
);

CREATE TABLE enrollment_tokens (
    id                BINARY(16)    NOT NULL PRIMARY KEY,
    token_hash        VARBINARY(64) NOT NULL,
    created_by        BINARY(16)    NOT NULL,
    expires_at        DATETIME(6)   NOT NULL,
    used_at           DATETIME(6)   NULL,
    used_by_server_id BINARY(16)    NULL,
    created_at        DATETIME(6)   NOT NULL,
    updated_at        DATETIME(6)   NOT NULL,
    UNIQUE KEY uq_enrollment_tokens_hash (token_hash),
    CONSTRAINT fk_enrollment_tokens_creator
        FOREIGN KEY (created_by) REFERENCES users (id),
    CONSTRAINT fk_enrollment_tokens_server
        FOREIGN KEY (used_by_server_id) REFERENCES servers (id)
);

-- ─── Deployments ────────────────────────────────────────────────────

CREATE TABLE deployments (
    id                         BINARY(16)    NOT NULL PRIMARY KEY,
    gpu_slot_id                BINARY(16)    NOT NULL,
    server_id                  BINARY(16)    NOT NULL,
    registry_tag_id            BINARY(16)    NOT NULL,
    gitlab_instance_id         BINARY(16)    NOT NULL,
    image_ref                  VARCHAR(1024) NOT NULL,
    created_by                 BINARY(16)    NOT NULL,
    state                      VARCHAR(32)   NOT NULL DEFAULT 'PENDING',
    container_id               VARCHAR(128)  NULL,
    container_name             VARCHAR(255)  NULL,
    replaced_by_deployment_id  BINARY(16)    NULL,
    error_message              TEXT          NULL,
    started_at                 DATETIME(6)   NULL,
    stopped_at                 DATETIME(6)   NULL,
    created_at                 DATETIME(6)   NOT NULL,
    updated_at                 DATETIME(6)   NOT NULL,
    KEY idx_deployments_slot (gpu_slot_id),
    KEY idx_deployments_server (server_id),
    KEY idx_deployments_state (state),
    KEY idx_deployments_created (created_at),
    CONSTRAINT fk_deployments_slot
        FOREIGN KEY (gpu_slot_id) REFERENCES gpu_slots (id),
    CONSTRAINT fk_deployments_server
        FOREIGN KEY (server_id) REFERENCES servers (id),
    CONSTRAINT fk_deployments_tag
        FOREIGN KEY (registry_tag_id) REFERENCES registry_tags (id),
    CONSTRAINT fk_deployments_instance
        FOREIGN KEY (gitlab_instance_id) REFERENCES gitlab_instances (id),
    CONSTRAINT fk_deployments_creator
        FOREIGN KEY (created_by) REFERENCES users (id),
    CONSTRAINT fk_deployments_replaced_by
        FOREIGN KEY (replaced_by_deployment_id) REFERENCES deployments (id)
);

-- Append-only: application code only ever INSERTs here.
CREATE TABLE deployment_events (
    id            BINARY(16)  NOT NULL PRIMARY KEY,
    deployment_id BINARY(16)  NOT NULL,
    from_state    VARCHAR(32) NULL,
    to_state      VARCHAR(32) NOT NULL,
    actor_type    VARCHAR(16) NOT NULL,
    actor_id      BINARY(16)  NULL,
    detail        JSON        NULL,
    created_at    DATETIME(6) NOT NULL,
    KEY idx_deployment_events_deployment (deployment_id, created_at),
    CONSTRAINT fk_deployment_events_deployment
        FOREIGN KEY (deployment_id) REFERENCES deployments (id)
);

CREATE TABLE deployment_ports (
    id             BINARY(16)        NOT NULL PRIMARY KEY,
    deployment_id  BINARY(16)        NOT NULL,
    container_port SMALLINT UNSIGNED NOT NULL,
    host_port      SMALLINT UNSIGNED NOT NULL,
    protocol       VARCHAR(8)        NOT NULL DEFAULT 'tcp',
    created_at     DATETIME(6)       NOT NULL,
    KEY idx_deployment_ports_deployment (deployment_id),
    CONSTRAINT fk_deployment_ports_deployment
        FOREIGN KEY (deployment_id) REFERENCES deployments (id)
);

-- env_value is VARBINARY: encrypted by the app when is_secret=1,
-- UTF-8 bytes otherwise. Masked in UI and logs either way when secret.
CREATE TABLE deployment_env (
    id            BINARY(16)      NOT NULL PRIMARY KEY,
    deployment_id BINARY(16)      NOT NULL,
    env_key       VARCHAR(255)    NOT NULL,
    env_value     VARBINARY(8192) NOT NULL,
    is_secret     TINYINT(1)      NOT NULL DEFAULT 0,
    created_at    DATETIME(6)     NOT NULL,
    UNIQUE KEY uq_deployment_env_key (deployment_id, env_key),
    CONSTRAINT fk_deployment_env_deployment
        FOREIGN KEY (deployment_id) REFERENCES deployments (id)
);

CREATE TABLE deployment_volumes (
    id             BINARY(16)    NOT NULL PRIMARY KEY,
    deployment_id  BINARY(16)    NOT NULL,
    host_path      VARCHAR(1024) NOT NULL,
    container_path VARCHAR(1024) NOT NULL,
    read_only      TINYINT(1)    NOT NULL DEFAULT 0,
    created_at     DATETIME(6)   NOT NULL,
    KEY idx_deployment_volumes_deployment (deployment_id),
    CONSTRAINT fk_deployment_volumes_deployment
        FOREIGN KEY (deployment_id) REFERENCES deployments (id)
);

-- ─── Agent task queue ───────────────────────────────────────────────

CREATE TABLE agent_tasks (
    id            BINARY(16)  NOT NULL PRIMARY KEY,
    server_id     BINARY(16)  NOT NULL,
    deployment_id BINARY(16)  NULL,
    task_type     VARCHAR(32) NOT NULL,
    payload       JSON        NOT NULL,
    state         VARCHAR(16) NOT NULL DEFAULT 'QUEUED',
    dispatched_at DATETIME(6) NULL,
    completed_at  DATETIME(6) NULL,
    attempts      INT         NOT NULL DEFAULT 0,
    created_at    DATETIME(6) NOT NULL,
    updated_at    DATETIME(6) NOT NULL,
    KEY idx_agent_tasks_server_state (server_id, state),
    KEY idx_agent_tasks_deployment (deployment_id),
    CONSTRAINT fk_agent_tasks_server
        FOREIGN KEY (server_id) REFERENCES servers (id),
    CONSTRAINT fk_agent_tasks_deployment
        FOREIGN KEY (deployment_id) REFERENCES deployments (id)
);

CREATE TABLE agent_task_results (
    id            BINARY(16) NOT NULL PRIMARY KEY,
    agent_task_id BINARY(16) NOT NULL,
    success       TINYINT(1) NOT NULL,
    detail        JSON       NULL,
    logs_excerpt  TEXT       NULL,
    reported_at   DATETIME(6) NOT NULL,
    KEY idx_agent_task_results_task (agent_task_id),
    CONSTRAINT fk_agent_task_results_task
        FOREIGN KEY (agent_task_id) REFERENCES agent_tasks (id)
);

-- ─── Audit ──────────────────────────────────────────────────────────

-- Append-only: application code only ever INSERTs here.
CREATE TABLE audit_logs (
    id           BINARY(16)   NOT NULL PRIMARY KEY,
    actor_type   VARCHAR(16)  NOT NULL,
    actor_id     BINARY(16)   NULL,
    action       VARCHAR(64)  NOT NULL,
    subject_type VARCHAR(64)  NULL,
    subject_id   BINARY(16)   NULL,
    detail       JSON         NULL,
    ip_address   VARCHAR(45)  NULL,
    created_at   DATETIME(6)  NOT NULL,
    KEY idx_audit_logs_created (created_at),
    KEY idx_audit_logs_actor (actor_type, actor_id),
    KEY idx_audit_logs_action (action)
);
