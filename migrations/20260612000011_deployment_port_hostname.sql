-- HTTP/HTTPS publishing (operator: *.ai.protv.ro, agent-managed nginx).
-- Each HTTP/HTTPS port gets a hostname under the apps domain; the agent
-- writes a local nginx vhost proxying that hostname to the container's
-- host port. TCP/UDP ports leave this NULL.

ALTER TABLE deployment_ports
    ADD COLUMN hostname VARCHAR(255) NULL AFTER kind;
