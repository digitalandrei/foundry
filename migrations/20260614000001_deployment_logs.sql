-- Phase 7 (Logs): per-deployment container log capture. The agent ships
-- incremental stdout+stderr for each managed running container; the
-- controller keeps a bounded window — at most 7 days, and at most a
-- fixed number of newest chunks per deployment so a log-spamming
-- container cannot exhaust the controller. Rows are deleted when the
-- deployment is removed/dismissed (see lifecycle::transition_deployment).

CREATE TABLE IF NOT EXISTS deployment_logs (
    id            BINARY(16)   NOT NULL PRIMARY KEY,
    deployment_id BINARY(16)   NOT NULL,
    server_id     BINARY(16)   NOT NULL,
    container_id  VARCHAR(128)     NULL,
    -- Newest docker log timestamp in the chunk (retention clock).
    logged_at     DATETIME(6)  NOT NULL,
    -- Merged stdout+stderr, docker `--timestamps` lines, chronological.
    content       MEDIUMTEXT   NOT NULL,
    KEY idx_deployment_logs_series (deployment_id, logged_at),
    KEY idx_deployment_logs_sweep (logged_at),
    CONSTRAINT fk_deployment_logs_deployment
        FOREIGN KEY (deployment_id) REFERENCES deployments (id)
) ENGINE=InnoDB;
