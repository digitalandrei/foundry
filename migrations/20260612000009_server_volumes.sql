-- Persistent storage (operator requirement, Phase 6): named per-server
-- volumes living under /storage/containers/<name>. Lifecycle is
-- independent of deployments — removing a container keeps its data;
-- a volume can later be mounted into another container. Deletion is
-- explicit (REMOVE_VOLUME agent task).

CREATE TABLE server_volumes (
    id         BINARY(16)   NOT NULL PRIMARY KEY,
    server_id  BINARY(16)   NOT NULL,
    name       VARCHAR(63)  NOT NULL,
    path       VARCHAR(255) NOT NULL,
    created_by BINARY(16)   NOT NULL,
    created_at DATETIME(6)  NOT NULL,
    updated_at DATETIME(6)  NOT NULL,
    UNIQUE KEY uq_server_volumes_name (server_id, name),
    CONSTRAINT fk_server_volumes_server FOREIGN KEY (server_id) REFERENCES servers (id),
    CONSTRAINT fk_server_volumes_creator FOREIGN KEY (created_by) REFERENCES users (id)
);

ALTER TABLE deployment_volumes
    ADD COLUMN server_volume_id BINARY(16) NULL AFTER deployment_id,
    ADD CONSTRAINT fk_deployment_volumes_volume
        FOREIGN KEY (server_volume_id) REFERENCES server_volumes (id);
