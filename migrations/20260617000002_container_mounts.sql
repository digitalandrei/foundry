-- Volume mounts observed on every container in the inventory snapshot
-- (JSON array, like `ports`/`gpu_uuids`). Lets operators see what a
-- pre-running / unmanaged container has bind-mounted before adopting it.

ALTER TABLE server_containers
    ADD COLUMN mounts LONGTEXT NULL AFTER gpu_uuids;
