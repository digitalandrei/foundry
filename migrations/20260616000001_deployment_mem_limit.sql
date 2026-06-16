-- Optional per-deployment Docker memory cap (operator slider on the
-- deploy dialog: 32–256 GB, or unlimited). NULL = unlimited (the
-- default — existing rows and untouched deploys keep no Foundry-set
-- cap). Stored in MB; applied by the agent as the container's
-- `--memory` HostConfig limit only when set.
ALTER TABLE deployments
    ADD COLUMN mem_limit_mb INT UNSIGNED NULL AFTER container_name;
