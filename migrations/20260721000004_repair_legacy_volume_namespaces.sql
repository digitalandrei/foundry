-- Migration 00003 was deliberately expand-compatible with the old
-- controller by defaulting new rows to `legacy`. Repair rows created during
-- that rolling window from the earliest deployment that actually mounted
-- them. A conflicting canonical row is left untouched for operator review;
-- neither identity nor filesystem data is ever merged implicitly.

CREATE TEMPORARY TABLE volume_namespace_repairs (
    volume_id BINARY(16) NOT NULL PRIMARY KEY,
    project_name VARCHAR(63) NOT NULL
);

INSERT INTO volume_namespace_repairs (volume_id, project_name)
SELECT ranked.server_volume_id, ranked.container_name
FROM (
    SELECT dv.server_volume_id,
           d.container_name,
           ROW_NUMBER() OVER (
               PARTITION BY dv.server_volume_id
               ORDER BY d.created_at, d.id
           ) AS namespace_rank
    FROM deployment_volumes dv
    JOIN deployments d ON d.id = dv.deployment_id
    JOIN server_volumes v ON v.id = dv.server_volume_id
    WHERE v.project_name = 'legacy'
      AND d.container_name IS NOT NULL
) ranked
WHERE ranked.namespace_rank = 1;

DELETE repair
FROM volume_namespace_repairs repair
JOIN server_volumes legacy_volume ON legacy_volume.id = repair.volume_id
JOIN server_volumes conflict
  ON conflict.id <> legacy_volume.id
 AND conflict.server_id = legacy_volume.server_id
 AND conflict.placement = legacy_volume.placement
 AND conflict.placement_id = legacy_volume.placement_id
 AND conflict.project_name = repair.project_name
 AND conflict.name = legacy_volume.name;

UPDATE server_volumes volume
JOIN volume_namespace_repairs repair ON repair.volume_id = volume.id
SET volume.project_name = repair.project_name,
    volume.updated_at = UTC_TIMESTAMP(6)
WHERE volume.project_name = 'legacy';

UPDATE server_volumes volume
LEFT JOIN volume_namespace_repairs repair ON repair.volume_id = volume.id
SET volume.project_name = CONCAT('_legacy-', LOWER(HEX(volume.id))),
    volume.updated_at = UTC_TIMESTAMP(6)
WHERE volume.project_name = 'legacy'
  AND repair.volume_id IS NULL;

DROP TEMPORARY TABLE volume_namespace_repairs;

-- Linux paths and Rust's explicit reuse check are case-sensitive; make the
-- canonical database key agree. Omitting DEFAULT also closes the rolling
-- migration escape hatch now that the new controller always supplies it.
ALTER TABLE server_volumes
    MODIFY COLUMN name VARCHAR(63) COLLATE utf8mb4_bin NOT NULL,
    MODIFY COLUMN project_name VARCHAR(63) COLLATE utf8mb4_bin NOT NULL,
    MODIFY COLUMN path VARCHAR(255) COLLATE utf8mb4_bin NOT NULL;
