-- Docker daemon visibility per server (Phase 5): version is already a
-- column; add live container counts reported by the agent's inventory.

ALTER TABLE servers
    ADD COLUMN containers_running INT NULL AFTER docker_version,
    ADD COLUMN containers_total INT NULL AFTER containers_running;
