-- Adopt externally-created (non-Foundry) containers into a deployment
-- record so operators get the same control surface — logs, console/bash,
-- stop, delete, replace — as managed containers. An adopted deployment
-- wraps a container the agent did NOT create: it is resolved by docker id
-- (`adopted_container_id`), not by the foundry.managed/foundry.deployment_id
-- labels, and has no registry origin — so the registry columns become
-- nullable. Adoption requires the container to occupy a GPU slot, so
-- `gpu_slot_id` stays NOT NULL.

ALTER TABLE deployments
    ADD COLUMN adopted_container_id VARCHAR(128) NULL AFTER container_name,
    MODIFY COLUMN registry_tag_id    BINARY(16) NULL,
    MODIFY COLUMN gitlab_instance_id BINARY(16) NULL;
