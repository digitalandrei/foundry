-- GPU groups (aggregation, 1 container : N GPUs) + multi-use slots
-- (sharing, N containers : 1 GPU). See docs/DATABASE.md and
-- docs/ARCHITECTURE.md § Multi-slot occupancy.
--
-- Two orthogonal capabilities, both operator/admin config:
--   * gpu_groups / gpu_group_members — a named set of whole GPUs on one
--     server; deploying to it runs one container across all members.
--   * gpu_slots.max_occupants — per-slot concurrency cap (1 = single-use,
--     >1 = soft sharing, no VRAM isolation).
-- deployment_slots generalises deployment→slot to many: occupancy is now
-- the count of active rows pointing at a slot (multi-use falls out for
-- free), and a group deploy holds one row per member.

-- ─── GPU groups ─────────────────────────────────────────────────────

CREATE TABLE gpu_groups (
    id         BINARY(16)  NOT NULL PRIMARY KEY,
    server_id  BINARY(16)  NOT NULL,
    name       VARCHAR(64) NOT NULL,
    created_by BINARY(16)  NOT NULL,
    created_at DATETIME(6) NOT NULL,
    updated_at DATETIME(6) NOT NULL,
    UNIQUE KEY uq_gpu_groups_name (server_id, name),
    KEY idx_gpu_groups_server (server_id),
    CONSTRAINT fk_gpu_groups_server
        FOREIGN KEY (server_id) REFERENCES servers (id),
    CONSTRAINT fk_gpu_groups_creator
        FOREIGN KEY (created_by) REFERENCES users (id)
);

CREATE TABLE gpu_group_members (
    group_id BINARY(16) NOT NULL,
    gpu_id   BINARY(16) NOT NULL,
    PRIMARY KEY (group_id, gpu_id),   -- a GPU can't repeat within a group
    KEY idx_member_gpu (gpu_id),      -- reverse: which/how many groups a GPU is in
    CONSTRAINT fk_gpu_group_members_group
        FOREIGN KEY (group_id) REFERENCES gpu_groups (id) ON DELETE CASCADE,
    CONSTRAINT fk_gpu_group_members_gpu
        FOREIGN KEY (gpu_id) REFERENCES gpus (id)
);

-- ─── Multi-slot occupancy (source of truth) ─────────────────────────

CREATE TABLE deployment_slots (
    deployment_id BINARY(16) NOT NULL,
    gpu_slot_id   BINARY(16) NOT NULL,
    PRIMARY KEY (deployment_id, gpu_slot_id),
    KEY idx_deployment_slots_slot (gpu_slot_id),
    CONSTRAINT fk_deployment_slots_deployment
        FOREIGN KEY (deployment_id) REFERENCES deployments (id),
    CONSTRAINT fk_deployment_slots_slot
        FOREIGN KEY (gpu_slot_id) REFERENCES gpu_slots (id)
);

-- Multi-use: per-slot concurrency cap. 1 = single-use (back-compat).
-- Capped at 4 (operator decision 2026-06-16) so a typo can't
-- oversubscribe a card into uselessness; also enforced in the DTO.
ALTER TABLE gpu_slots
    ADD COLUMN max_occupants INT UNSIGNED NOT NULL DEFAULT 1
        CHECK (max_occupants BETWEEN 1 AND 4);

-- A group deploy stamps every member's deployment with the group id;
-- NULL = single-GPU deploy. gpu_slot_id stays the denormalised primary
-- (first/only member) so existing single-slot queries keep working;
-- deployment_slots is authoritative for occupancy.
ALTER TABLE deployments
    ADD COLUMN gpu_group_id BINARY(16) NULL,
    ADD KEY idx_deployments_group (gpu_group_id),
    ADD CONSTRAINT fk_deployments_group
        FOREIGN KEY (gpu_group_id) REFERENCES gpu_groups (id);

-- Backfill occupancy from the denormalised primary slot for every
-- deployment that currently holds its slot (same active predicate the
-- port allocator uses) so occupancy queries can switch over atomically.
INSERT INTO deployment_slots (deployment_id, gpu_slot_id)
SELECT id, gpu_slot_id FROM deployments
WHERE state IN ('PENDING','VALIDATING','PULLING_IMAGE','CREATING_CONTAINER',
                'STARTING','RUNNING','STOPPING','STOPPED','RESTARTING','REMOVING')
   OR (state = 'FAILED' AND container_id IS NOT NULL);
