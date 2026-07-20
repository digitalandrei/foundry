-- Only one non-terminal deployment may wrap a given external container.
-- MariaDB UNIQUE indexes allow multiple NULLs, so terminal and ordinary
-- deployments deliberately project NULL while active adoptions project a
-- stable server/container key.

ALTER TABLE deployments
    ADD COLUMN active_adoption_key VARCHAR(161)
        GENERATED ALWAYS AS (
            CASE
                WHEN adopted_container_id IS NOT NULL
                 AND state NOT IN ('STOPPED', 'REMOVED', 'FAILED', 'REPLACED')
                THEN CONCAT(HEX(server_id), ':', adopted_container_id)
                ELSE NULL
            END
        ) STORED,
    ADD UNIQUE KEY uq_deployments_active_adoption (active_adoption_key);
