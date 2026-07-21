-- Add the user-given deployment/container name as the storage namespace.
-- This is deliberately unrelated to GitLab projects. The resulting logical
-- identity is:
--   server / (physical slot, GPU-group target, or shared server) /
--   deployment name / mount name
-- Existing paths and data remain untouched; only new volumes use the matching
-- directory hierarchy on disk.

ALTER TABLE server_volumes
    ADD COLUMN project_name VARCHAR(63) NOT NULL DEFAULT 'legacy' AFTER placement_id;

UPDATE server_volumes v
SET v.project_name = COALESCE((
    SELECT d.container_name
    FROM deployment_volumes dv
    JOIN deployments d ON d.id = dv.deployment_id
    WHERE dv.server_volume_id = v.id
      AND d.container_name IS NOT NULL
    ORDER BY d.created_at, d.id
    LIMIT 1
), 'legacy');

ALTER TABLE server_volumes
    DROP KEY uq_server_volumes_placement_name,
    ADD UNIQUE KEY uq_server_volumes_project_mount
        (server_id, placement, placement_id, project_name, name);
