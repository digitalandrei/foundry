-- Local (non-GitLab) operator accounts (docs/SECURITY.md § Identity).
-- Purpose: portal administration independent of any GitLab instance —
-- bootstrap, instance onboarding, operations. Local accounts carry no
-- GitLab identity, so they see no projects/registries and cannot
-- deploy; authorization for GitLab resources still comes only from
-- GitLab accounts.

CREATE TABLE local_credentials (
    user_id       BINARY(16)   NOT NULL PRIMARY KEY,
    username      VARCHAR(64)  NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    created_at    DATETIME(6)  NOT NULL,
    updated_at    DATETIME(6)  NOT NULL,
    UNIQUE KEY uq_local_credentials_username (username),
    CONSTRAINT fk_local_credentials_user
        FOREIGN KEY (user_id) REFERENCES users (id)
);
