-- Fleet auto-enrollment: a reusable, time-limited key (kind='FLEET') an
-- agent presents at first boot. Unlike SERVER tokens it is not bound to a
-- pre-created server and may enrol many hosts within its TTL / use budget;
-- the agent's hostname becomes the server identity, so hostname is unique.

ALTER TABLE enrollment_tokens
    ADD COLUMN kind     VARCHAR(16)  NOT NULL DEFAULT 'SERVER' AFTER server_id,
    ADD COLUMN max_uses INT UNSIGNED NULL                      AFTER kind,
    ADD COLUMN uses     INT UNSIGNED NOT NULL DEFAULT 0         AFTER max_uses;

-- Hostname is the fleet identity → unique. Make it nullable first so the
-- name-first (un-enrolled) servers don't collide on '' under the index.
UPDATE servers SET hostname = NULL WHERE hostname = '';
ALTER TABLE servers MODIFY COLUMN hostname VARCHAR(255) NULL;
ALTER TABLE servers ADD UNIQUE KEY uq_servers_hostname (hostname);
