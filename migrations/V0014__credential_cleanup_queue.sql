-- 系统凭据库无法参与 SQLite 事务；这里只持久化待删除引用，不保存任何秘密值。
CREATE TABLE credential_cleanup_queue (
    credential_reference TEXT PRIMARY KEY
        CHECK (length(credential_reference) BETWEEN 1 AND 512),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 4294967295),
    next_attempt_at_unix_ms INTEGER NOT NULL CHECK (next_attempt_at_unix_ms >= 0),
    last_error_code TEXT,
    created_at_unix_ms INTEGER NOT NULL CHECK (created_at_unix_ms >= 0),
    updated_at_unix_ms INTEGER NOT NULL CHECK (updated_at_unix_ms >= created_at_unix_ms),
    CHECK (
        last_error_code IS NULL
        OR (
            length(last_error_code) BETWEEN 4 AND 128
            AND substr(last_error_code, 1, 3) = 'NP_'
            AND last_error_code NOT GLOB '*[^A-Z0-9_]*'
        )
    )
) STRICT;

CREATE INDEX credential_cleanup_due
ON credential_cleanup_queue(next_attempt_at_unix_ms, credential_reference);
