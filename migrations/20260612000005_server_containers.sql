-- Observed Docker containers per server (full snapshot per inventory
-- upload; replace-all semantics). Read-only visibility — Foundry only
-- ever MANAGES containers labeled foundry.managed=true.

CREATE TABLE server_containers (
    id           BINARY(16)    NOT NULL PRIMARY KEY,
    server_id    BINARY(16)    NOT NULL,
    container_id VARCHAR(128)  NOT NULL,
    name         VARCHAR(255)  NOT NULL,
    image        VARCHAR(1024) NOT NULL,
    state        VARCHAR(32)   NOT NULL,
    status       VARCHAR(255)  NOT NULL,
    managed      TINYINT(1)    NOT NULL DEFAULT 0,
    reported_at  DATETIME(6)   NOT NULL,
    KEY idx_server_containers_server (server_id),
    CONSTRAINT fk_server_containers_server
        FOREIGN KEY (server_id) REFERENCES servers (id)
);
