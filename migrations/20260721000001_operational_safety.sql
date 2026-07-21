-- Foundry 0.59.0: host readiness, immutable deploys, application policy,
-- storage accounting/quotas, and per-application traffic observability.

ALTER TABLE servers
    ADD COLUMN setup_revision INT UNSIGNED NULL AFTER docker_ok,
    ADD COLUMN readiness_json JSON NULL AFTER setup_revision,
    ADD COLUMN readiness_checked_at DATETIME NULL AFTER readiness_json,
    ADD COLUMN storage_total_bytes BIGINT UNSIGNED NULL AFTER readiness_checked_at,
    ADD COLUMN storage_available_bytes BIGINT UNSIGNED NULL AFTER storage_total_bytes;

ALTER TABLE deployments
    ADD COLUMN image_digest VARCHAR(80) NULL AFTER image_ref,
    ADD COLUMN health_status VARCHAR(32) NULL AFTER error_message,
    ADD COLUMN health_detail TEXT NULL AFTER health_status;

ALTER TABLE deployment_ports
    ADD COLUMN is_primary TINYINT(1) NOT NULL DEFAULT 0 AFTER hostname,
    ADD COLUMN health_path VARCHAR(1024) NULL AFTER is_primary,
    ADD COLUMN max_body_size_bytes BIGINT UNSIGNED NOT NULL DEFAULT 2147483648 AFTER health_path,
    ADD COLUMN proxy_timeout_seconds INT UNSIGNED NOT NULL DEFAULT 300 AFTER max_body_size_bytes;

ALTER TABLE server_volumes
    ADD COLUMN used_bytes BIGINT UNSIGNED NULL AFTER path,
    ADD COLUMN quota_bytes BIGINT UNSIGNED NULL AFTER used_bytes,
    ADD COLUMN usage_measured_at DATETIME NULL AFTER quota_bytes;

CREATE TABLE app_access_logs (
    id BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    deployment_id BINARY(16) NOT NULL,
    occurred_at DATETIME(3) NOT NULL,
    method VARCHAR(16) NOT NULL,
    path VARCHAR(2048) NOT NULL,
    status SMALLINT UNSIGNED NOT NULL,
    request_time_ms INT UNSIGNED NOT NULL,
    response_bytes BIGINT UNSIGNED NOT NULL,
    request_id VARCHAR(64) NULL,
    PRIMARY KEY (id),
    KEY idx_app_access_deployment_time (deployment_id, occurred_at),
    KEY idx_app_access_time (occurred_at),
    UNIQUE KEY uq_app_access_deployment_request (deployment_id, request_id),
    CONSTRAINT fk_app_access_deployment FOREIGN KEY (deployment_id)
        REFERENCES deployments(id) ON DELETE CASCADE
) ENGINE=InnoDB;
