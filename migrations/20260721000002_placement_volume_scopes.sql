-- Persistent storage is placement-owned, never GitLab-project-owned:
--   SLOT   = reusable by deployments landing in one physical/GPU-group slot
--   SERVER = reusable by deployments in any slot on the server
-- Existing host paths and data are preserved. Former project scopes can
-- collide after project identity is removed, so deterministically rename
-- every later duplicate before installing the new placement identity key.

ALTER TABLE server_volumes
    ADD COLUMN gpu_group_id BINARY(16) NULL AFTER gpu_slot_id,
    ADD KEY idx_server_volumes_group (gpu_group_id),
    ADD CONSTRAINT fk_server_volumes_group
        FOREIGN KEY (gpu_group_id) REFERENCES gpu_groups (id);

-- A GPU group is a deployable slot of its own. Move a legacy SLOT volume
-- away from the first physical member when every attachment belongs to the
-- same group. Mixed-use volumes retain their original physical-slot scope.
UPDATE server_volumes v
JOIN (
    SELECT dv.server_volume_id, MIN(d.gpu_group_id) AS gpu_group_id
    FROM deployment_volumes dv
    JOIN deployments d ON d.id = dv.deployment_id
    WHERE d.gpu_group_id IS NOT NULL
      AND NOT EXISTS (
          SELECT 1 FROM deployment_volumes individual_dv
          JOIN deployments individual_d ON individual_d.id = individual_dv.deployment_id
          WHERE individual_dv.server_volume_id = dv.server_volume_id
            AND individual_d.gpu_group_id IS NULL
      )
    GROUP BY dv.server_volume_id
    HAVING COUNT(DISTINCT d.gpu_group_id) = 1
) grouped ON grouped.server_volume_id = v.id
SET v.placement_id = grouped.gpu_group_id,
    v.gpu_slot_id = NULL,
    v.gpu_group_id = grouped.gpu_group_id
WHERE v.placement = 'SLOT';

UPDATE server_volumes v
JOIN (
    SELECT DISTINCT later.id
    FROM server_volumes later
    JOIN server_volumes earlier
      ON earlier.server_id = later.server_id
     AND earlier.placement = later.placement
     AND earlier.placement_id = later.placement_id
     AND earlier.name = later.name
     AND earlier.id < later.id
) duplicate_volume ON duplicate_volume.id = v.id
SET v.name = CONCAT(LEFT(v.name, 30), '-', LOWER(HEX(v.id)));

ALTER TABLE server_volumes
    DROP FOREIGN KEY fk_server_volumes_project,
    DROP KEY idx_server_volumes_project,
    DROP KEY uq_server_volumes_scope,
    DROP COLUMN gitlab_project_id,
    DROP COLUMN visibility,
    DROP COLUMN scope_id,
    DROP COLUMN owner_slug,
    ADD UNIQUE KEY uq_server_volumes_placement_name
        (server_id, placement, placement_id, name);
