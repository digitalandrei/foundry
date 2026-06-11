-- Per-user volume namespacing (operator refinement): paths live at
-- /storage/containers/<owner_slug>/<name>; names are unique per
-- (server, owner); paths unique per server guard against slug
-- collisions. Users see only their own volumes (admins see all).

ALTER TABLE server_volumes
    DROP KEY uq_server_volumes_name,
    ADD COLUMN owner_slug VARCHAR(63) NOT NULL AFTER name,
    ADD UNIQUE KEY uq_server_volumes_owner_name (server_id, created_by, name),
    ADD UNIQUE KEY uq_server_volumes_path (server_id, path);
