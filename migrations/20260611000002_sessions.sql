-- Server-side sessions (docs/SECURITY.md § Identity & Sessions).
-- The cookie carries a random token; only its SHA-256 lands here, so a
-- database leak does not yield usable session credentials.

CREATE TABLE sessions (
    id          BINARY(16)    NOT NULL PRIMARY KEY,
    token_hash  VARBINARY(32) NOT NULL,
    user_id     BINARY(16)    NOT NULL,
    ip_address  VARCHAR(45)   NULL,
    user_agent  VARCHAR(255)  NULL,
    expires_at  DATETIME(6)   NOT NULL,
    created_at  DATETIME(6)   NOT NULL,
    UNIQUE KEY uq_sessions_token_hash (token_hash),
    KEY idx_sessions_expires (expires_at),
    CONSTRAINT fk_sessions_user
        FOREIGN KEY (user_id) REFERENCES users (id)
);
