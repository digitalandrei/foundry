-- GPU/MIG device UUIDs a container is bound to (resolved by the agent
-- from the container's device requests / NVIDIA_VISIBLE_DEVICES). JSON
-- array; lets the dashboard map even non-Foundry containers onto the
-- slot whose GPU they occupy.
ALTER TABLE server_containers ADD COLUMN gpu_uuids LONGTEXT NULL AFTER ports;
