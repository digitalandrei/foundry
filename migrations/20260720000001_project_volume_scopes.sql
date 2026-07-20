-- General persistent-storage scopes:
--   visibility: PRIVATE (creator only) or PROJECT (all live GitLab members)
--   placement:  SERVER (portable across its slots) or SLOT (one physical slot)
-- Host paths are opaque IDs for new volumes; legacy paths remain valid.

ALTER TABLE server_volumes
    DROP KEY uq_server_volumes_owner_name,
    ADD COLUMN gitlab_project_id BINARY(16) NULL AFTER server_id,
    ADD COLUMN visibility VARCHAR(16) NOT NULL DEFAULT 'PRIVATE' AFTER name,
    ADD COLUMN placement VARCHAR(16) NOT NULL DEFAULT 'SERVER' AFTER visibility,
    ADD COLUMN scope_id BINARY(16) NULL AFTER placement,
    ADD COLUMN placement_id BINARY(16) NULL AFTER scope_id,
    ADD COLUMN gpu_slot_id BINARY(16) NULL AFTER placement_id;

-- Attach legacy volumes to the project of their oldest deployment when
-- possible. Unattached legacy volumes stay creator-private and remain
-- manageable, but cannot be selected for a new project until recreated.
UPDATE server_volumes v
SET v.gitlab_project_id = (
    SELECT r.gitlab_project_id
    FROM deployment_volumes dv
    JOIN deployments d ON d.id = dv.deployment_id
    JOIN registry_tags t ON t.id = d.registry_tag_id
    JOIN registry_repositories r ON r.id = t.registry_repository_id
    WHERE dv.server_volume_id = v.id
    ORDER BY d.created_at
    LIMIT 1
);

UPDATE server_volumes
SET scope_id = created_by,
    placement_id = server_id;

ALTER TABLE server_volumes
    MODIFY scope_id BINARY(16) NOT NULL,
    MODIFY placement_id BINARY(16) NOT NULL,
    ADD KEY idx_server_volumes_project (gitlab_project_id),
    ADD KEY idx_server_volumes_slot (gpu_slot_id),
    ADD UNIQUE KEY uq_server_volumes_scope
        (server_id, gitlab_project_id, visibility, scope_id,
         placement, placement_id, name),
    ADD CONSTRAINT fk_server_volumes_project
        FOREIGN KEY (gitlab_project_id) REFERENCES gitlab_projects (id),
    ADD CONSTRAINT fk_server_volumes_slot
        FOREIGN KEY (gpu_slot_id) REFERENCES gpu_slots (id);

ALTER TABLE deployment_volumes
    ADD COLUMN purge_on_redeploy TINYINT(1) NOT NULL DEFAULT 0 AFTER read_only;
