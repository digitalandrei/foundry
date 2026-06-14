-- Per-server Docker daemon liveness (0.20.0): the agent reports whether
-- the Docker daemon answered. NULL = no inventory yet (unknown), like
-- nginx_status; true/false drive the "Docker active" indicator and the
-- deploy gate (a deploy onto a server with docker_ok = false is rejected
-- at create, the same way HTTP/S deploys are gated on app-publishing).
ALTER TABLE servers
    ADD COLUMN IF NOT EXISTS docker_ok BOOLEAN NULL AFTER docker_version;
