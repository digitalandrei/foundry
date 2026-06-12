-- App-publishing (nginx) readiness reported by the agent's inventory:
-- NULL = unknown (no recent snapshot), 1 = nginx + Foundry include in
-- place, 0 = nginx not installed (HTTP/S publishing unavailable).
ALTER TABLE servers ADD COLUMN app_publishing_ready TINYINT(1) NULL AFTER docker_version;
