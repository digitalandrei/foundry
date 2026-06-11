-- Named-agent enrollment (GitLab-agent style): a server is created
-- first in the UI; its enrollment token is bound to it. used_by_server_id
-- remains the consumption record (same server on success).

ALTER TABLE enrollment_tokens
    ADD COLUMN server_id BINARY(16) NULL AFTER token_hash,
    ADD KEY idx_enrollment_tokens_server (server_id),
    ADD CONSTRAINT fk_enrollment_tokens_target
        FOREIGN KEY (server_id) REFERENCES servers (id);
