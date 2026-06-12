-- The locking uniqueness probe in assign_hostnames must lock only
-- matching rows; without an index it full-scans deployment_ports with
-- next-key locks (review finding: cross-server create deadlocks).
ALTER TABLE deployment_ports ADD KEY idx_deployment_ports_hostname (hostname);
