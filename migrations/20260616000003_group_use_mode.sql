-- Group use-mode: a GPU group can be single-use (one exclusive container
-- across its GPUs — the default) or multi-use (shared by up to N
-- containers, soft sharing with no VRAM isolation), mirroring
-- gpu_slots.max_occupants. Capped 1–4 (operator decision).
ALTER TABLE gpu_groups
    ADD COLUMN max_occupants INT UNSIGNED NOT NULL DEFAULT 1
        CHECK (max_occupants BETWEEN 1 AND 4);
