-- Deterministic GPU ordering (operator request): persist the NVML
-- index from each snapshot; lists order by it. Display only — identity
-- remains the UUID.

ALTER TABLE gpus
    ADD COLUMN display_index INT UNSIGNED NOT NULL DEFAULT 0 AFTER gpu_uuid;
