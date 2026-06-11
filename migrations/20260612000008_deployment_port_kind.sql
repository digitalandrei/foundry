-- Port publishing kinds (plans/phase-06.md § Networking): HTTP/HTTPS
-- (central proxy, later build) vs TCP/UDP (direct on the server IP).

ALTER TABLE deployment_ports
    ADD COLUMN kind VARCHAR(8) NOT NULL DEFAULT 'TCP' AFTER protocol;
