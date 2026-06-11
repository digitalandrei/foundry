-- Telemetry (plans/phase-05.md § Telemetry extension): rolling metric
-- samples per server (JSON payload, 24h retention via sweeper) and
-- port mappings on the container snapshot.

CREATE TABLE server_metrics (
    id         BINARY(16)  NOT NULL PRIMARY KEY,
    server_id  BINARY(16)  NOT NULL,
    sampled_at DATETIME(6) NOT NULL,
    sample     JSON        NOT NULL,
    KEY idx_server_metrics_series (server_id, sampled_at),
    CONSTRAINT fk_server_metrics_server
        FOREIGN KEY (server_id) REFERENCES servers (id)
);

ALTER TABLE server_containers
    ADD COLUMN ports JSON NULL AFTER managed;
